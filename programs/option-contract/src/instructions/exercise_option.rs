use crate::{
    errors::{OptionError, TradingError},
    events::OptionExercised,
    math::{self, scaled_price_to_f64},
    state::{Contract, Custody, OptionDetail, OraclePrice, Pool, User},
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ExerciseOptionParams {
    pub option_index: u64,
    pub pool_name: String
}

pub fn exercise_option(ctx: Context<ExerciseOption>, params: &ExerciseOptionParams) -> Result<()> {
    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;
    let contract = &ctx.accounts.contract;
    let user = &mut ctx.accounts.user;
    let funding_account = &mut ctx.accounts.funding_account;
    let transfer_authority = &mut ctx.accounts.transfer_authority;
    let custody: &mut Box<Account<'_, Custody>> = &mut ctx.accounts.custody;
    let locked_custody = &mut ctx.accounts.locked_custody;
    let locked_custody_token_account = &mut ctx.accounts.locked_custody_token_account;
    let locked_oracle = &ctx.accounts.locked_oracle;
    let custody_oracle = &ctx.accounts.custody_oracle;

    // CRITICAL VALIDATION CHECKS - Add these at the beginning
    require_gte!(user.option_index, params.option_index);
    
    // Prevent re-exercising the same option
    require_eq!(
        option_detail.exercised,
        0,
        OptionError::OptionAlreadyExercised
    );
    
    // Ensure option is still valid
    require!(
        option_detail.valid,
        OptionError::OptionNotValid
    );
    
    // Verify option belongs to caller
    require_eq!(
        option_detail.owner,
        ctx.accounts.owner.key(),
        TradingError::InvalidOwner
    );

    // Current Unix timestamp
    let current_timestamp = contract.get_time()?;

    // Check if option is available to exercise, before expired time.
    require_gt!(
        option_detail.expired_date,
        current_timestamp,
        OptionError::InvalidTimeError
    );

    let token_price =
        OraclePrice::new_from_oracle(locked_oracle, current_timestamp, false)?;
    let sol_price =
        OraclePrice::new_from_oracle(custody_oracle, current_timestamp, false)?;
    let oracle_price = sol_price.get_price();

    require_gte!(
        locked_custody.token_locked,
        option_detail.amount,
        TradingError::InvalidLockedBalanceError
    );

    if custody.key() == locked_custody.key() {
        // call option
        let strike_price_f64 = scaled_price_to_f64(option_detail.strike_price)?;
        require_gte!(
            oracle_price,
            strike_price_f64,
            OptionError::InvalidPriceRequirementError
        );
        
        // Calculate profit amount for call option: (oracle_price - strike_price) * quantity
        // Using safe decimal math to handle precision properly
        let price_diff = math::checked_as_u64(oracle_price - strike_price_f64)?;
        let amount = math::checked_decimal_mul(
            price_diff,
            0, // oracle price exponent (assuming normalized)
            option_detail.quantity,
            0, // quantity exponent 
            -(custody.decimals as i32), // target token decimals
        )?;        

        // Use raw oracle price data instead of converted f64 to avoid precision loss
        require_gt!(token_price.price, 0, OptionError::InvalidPriceRequirementError);
        
        let profit_per_unit = math::checked_decimal_div(
            amount,
            -(custody.decimals as i32), // amount is already in target decimals
            token_price.price,
            token_price.exponent,
            -(custody.decimals as i32), // keep same precision
        )?;

        // Use the custody token account instead of custody metadata account
        contract.transfer_tokens(
            locked_custody_token_account.to_account_info(),
            funding_account.to_account_info(),
            transfer_authority.to_account_info(),
            token_program.to_account_info(),
            profit_per_unit,
        )?;

        option_detail.profit = profit_per_unit;
    } else {
        let strike_price_f64 = scaled_price_to_f64(option_detail.strike_price)?;
        require_gte!(
            strike_price_f64,
            oracle_price,
            OptionError::InvalidPriceRequirementError
        );

        // Calculate profit amount for put option: (strike_price - oracle_price) * quantity
        // Using safe decimal math to handle precision properly
        let price_diff = math::checked_as_u64(strike_price_f64 - oracle_price)?;
        let amount = math::checked_decimal_mul(
            price_diff,
            0, // oracle price exponent (assuming normalized)
            option_detail.quantity,
            0, // quantity exponent
            -(custody.decimals as i32), // target token decimals
        )?;
        require_gt!(token_price.price, 0, OptionError::InvalidPriceRequirementError);
        
        let profit_per_unit = math::checked_decimal_div(
            amount,
            -(custody.decimals as i32), // amount is already in target decimals
            token_price.price,
            token_price.exponent,
            -(locked_custody.decimals as i32), // keep same precision
        )?;

        // Use the custody token account instead of custody metadata account
        contract.transfer_tokens(
            locked_custody_token_account.to_account_info(),
            funding_account.to_account_info(),
            transfer_authority.to_account_info(),
            token_program.to_account_info(),
            profit_per_unit,
        )?;

        option_detail.profit = profit_per_unit;
    }

    // Mark option as exercised and invalid (these changes will now be saved!)
    option_detail.exercised = current_timestamp as u64;
    option_detail.valid = false;

    // Update locked custody balance
    locked_custody.token_locked =
        math::checked_sub(locked_custody.token_locked, option_detail.amount)?;

    emit!(OptionExercised {
        owner: option_detail.owner,
        index: option_detail.index,
        amount: option_detail.amount,
        quantity: option_detail.quantity,
        period: option_detail.period,
        expired_date: option_detail.expired_date,
        purchase_date: option_detail.purchase_date,
        option_type: option_detail.option_type,
        strike_price: option_detail.strike_price,
        valid: option_detail.valid,
        locked_asset: option_detail.locked_asset,
        pool: option_detail.pool,
        custody: option_detail.custody,
        premium: option_detail.premium,
        premium_asset: option_detail.premium_asset,
        limit_price: option_detail.limit_price,
        executed: option_detail.executed,
        entry_price: option_detail.entry_price,
        last_update_time: option_detail.last_update_time,
        take_profit_price: option_detail.take_profit_price,
        stop_loss_price: option_detail.stop_loss_price,
        exercised: option_detail.exercised,
        profit: option_detail.profit,
    });

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: ExerciseOptionParams)]
pub struct ExerciseOption<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = owner
    )]
    pub funding_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: empty PDA, authority for token accounts
    #[account(
        seeds = [b"transfer_authority"],
        bump = contract.transfer_authority_bump
    )]
    pub transfer_authority: AccountInfo<'info>,

    #[account(
        seeds = [b"contract"],
        bump = contract.bump
    )]
    pub contract: Box<Account<'info, Contract>>,

    #[account(
        mut,
        seeds = [b"pool", params.pool_name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    // MOVE ALL MINTS TO TOP BEFORE DEPENDENT ACCOUNTS
    #[account(mut)]
    pub custody_mint: Box<Account<'info, Mint>>,
    
    #[account(mut)]
    pub locked_custody_mint: Box<Account<'info, Mint>>,

    // NOW these accounts can derive correctly with mints available
    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody_mint.key().as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>, // Target price asset

    #[account(
        seeds = [b"user_v3", owner.key().as_ref()],
        bump,
    )]
    pub user: Box<Account<'info, User>>,

    // Add `mut` to option_detail!
    #[account(
        mut,  // Without this, changes aren't saved!
        seeds = [b"option", owner.key().as_ref(), 
                params.option_index.to_le_bytes().as_ref(),
                pool.key().as_ref(), custody.key().as_ref()],
        bump
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 locked_custody_mint.key().as_ref()],
        bump = locked_custody.bump,
        constraint = locked_custody.mint == locked_custody_mint.key() @ TradingError::InvalidMintError
    )]
    pub locked_custody: Box<Account<'info, Custody>>,

    // Token account can now be derived properly with mint available
    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 locked_custody_mint.key().as_ref()],
        bump,
        constraint = locked_custody_token_account.mint == locked_custody_mint.key() @ TradingError::InvalidMintError,
    )]
    pub locked_custody_token_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: oracle account for the position token
    #[account(
        constraint = locked_oracle.key() == locked_custody.oracle
    )]
    pub locked_oracle: AccountInfo<'info>,

    /// CHECK: oracle account for the solana token
    #[account(
        constraint = custody_oracle.key() == custody.oracle
    )]
    pub custody_oracle: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
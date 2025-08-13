use crate::{
    errors::{OptionError, TradingError},
    math::{self, scaled_price_to_f64},
    state::{Contract, Custody, OptionDetail, OraclePrice, Pool, User},
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token},
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AutoExerciseOptionParams {
    pub user: Pubkey,
    pub option_index: u64,
    pub pool_name: String,
}

pub fn auto_exercise(
    ctx: Context<AutoExerciseOption>,
    params: &AutoExerciseOptionParams,
) -> Result<()> {
    let option_detail = &mut ctx.accounts.option_detail;
    let contract = &ctx.accounts.contract;
    let user = &mut ctx.accounts.user;
    let custody: &mut Box<Account<'_, Custody>> = &mut ctx.accounts.custody;
    let locked_custody = &mut ctx.accounts.locked_custody;
    let locked_oracle = &ctx.accounts.locked_oracle;

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
    
    // Verify option belongs to the specified user
    require_eq!(
        option_detail.owner,
        params.user,
        TradingError::InvalidOwner
    );

    // Current Unix timestamp
    let current_timestamp = contract.get_time()?;

    // Auto-exercise should only work AFTER expiry (opposite of manual exercise)
    require_gte!(
        current_timestamp,
        option_detail.expired_date,
        OptionError::InvalidTimeError
    );

    let token_price =
        OraclePrice::new_from_oracle(locked_oracle, current_timestamp, false)?;
    let oracle_price = token_price.get_price();

    require_gte!(
        locked_custody.token_locked,
        option_detail.amount,
        TradingError::InvalidLockedBalanceError
    );

    if custody.key() == locked_custody.key() {
        // call option - only exercise if profitable
        let strike_price_f64 = scaled_price_to_f64(option_detail.strike_price)?;
        if oracle_price > strike_price_f64 {
            // Calculate Sol Amount from Option Detail Value : call / covered sol
            // Use more precise calculation to minimize rounding
            let price_diff = oracle_price - strike_price_f64;
            let intrinsic_value = price_diff * (option_detail.quantity as f64);
            let amount = intrinsic_value / oracle_price;

            option_detail.profit = math::checked_as_u64(amount.round())?;
            option_detail.claimed = math::checked_as_u64(amount.round())?;
        } else {
            // Option expired out of the money - no profit
            option_detail.claimed = 0;
            option_detail.profit = 0;
        }
    } else {
        // put option - only exercise if profitable
        let strike_price_f64 = scaled_price_to_f64(option_detail.strike_price)?;
        if strike_price_f64 > oracle_price {
            // Calculate Profit amount with option detail values: put / cash-secured usdc
            let price_diff = strike_price_f64 - oracle_price;
            let amount = price_diff * (option_detail.quantity as f64);

            option_detail.profit = math::checked_as_u64(amount.round())?;
            option_detail.claimed = amount as u64;
        } else {
            // Option expired out of the money - no profit
            option_detail.claimed = 0;
            option_detail.profit = 0;
        }
    }

    // Mark option as exercised and invalid
    option_detail.exercised = current_timestamp as u64;
    option_detail.valid = false;

    // Update locked custody balance
    locked_custody.token_locked =
        math::checked_sub(locked_custody.token_locked, option_detail.amount)?;

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: AutoExerciseOptionParams)]
pub struct AutoExerciseOption<'info> {
    #[account(mut)]
    pub tester: Signer<'info>,

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
        seeds = [b"user_v3", params.user.key().as_ref()],
        bump,
    )]
    pub user: Box<Account<'info, User>>,

    // Add `mut` to option_detail!
    #[account(
        mut,  // Without this, changes aren't saved!
        seeds = [b"option", params.user.key().as_ref(), 
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
    pub locked_custody: Box<Account<'info, Custody>>, // locked asset

    /// CHECK: oracle account for the position token
    #[account(
        constraint = locked_oracle.key() == locked_custody.oracle
    )]
    pub locked_oracle: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
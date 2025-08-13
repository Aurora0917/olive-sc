use crate::{
    errors::{OptionError, PoolError, TradingError},
    math::{self, f64_to_scaled_price, scaled_price_to_f64},
    utils::option_pricing::*,
    state::{Contract, Custody, OptionDetail, OraclePrice, Pool, User},
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer},
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct EditOptionParams {
    pub option_index: u64,
    pub pool_name: String,
    pub new_strike: Option<f64>,    // New strike price (None = keep current)
    pub new_expiry: Option<i64>,    // New expiry date (None = keep current)
    pub new_size: Option<f64>,      // New size in human-readable format (None = keep current)
    pub max_additional_premium: u64, // Slippage protection for additional payments
    pub min_refund_amount: u64,     // Slippage protection for refunds
}

pub fn edit_option(ctx: Context<EditOption>, params: &EditOptionParams) -> Result<()> {
    let owner = &ctx.accounts.owner;
    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;
    let contract = &ctx.accounts.contract;
    let user = &ctx.accounts.user;
    let _pool = &ctx.accounts.pool;
    let custody = &ctx.accounts.custody;
    let transfer_authority = &ctx.accounts.transfer_authority;

    let locked_custody = &mut ctx.accounts.locked_custody;
    let pay_custody = &mut ctx.accounts.pay_custody;
    let pay_custody_token_account = &ctx.accounts.pay_custody_token_account;
    let funding_account = &ctx.accounts.funding_account;
    let refund_account = &ctx.accounts.refund_account;

    let custody_oracle_account = &ctx.accounts.custody_oracle_account;
    let pay_custody_oracle_account = &ctx.accounts.pay_custody_oracle_account;

    // Validation checks
    require!(option_detail.valid, OptionError::OptionExpired);
    require_keys_eq!(option_detail.owner, owner.key());
    require_keys_eq!(option_detail.locked_asset, locked_custody.key());
    require_gte!(user.option_index, params.option_index);

    // Get current time and validate option hasn't expired
    let current_time = contract.get_time()?;
    require!(current_time < option_detail.expired_date, OptionError::InvalidTimeError);

    // Validate at least one parameter is being changed
    require!(
        params.new_strike.is_some() || params.new_expiry.is_some() || params.new_size.is_some(),
        TradingError::InvalidParameterError
    );

    // Get current oracle prices
    let underlying_price = OraclePrice::new_from_oracle(
        custody_oracle_account,
        current_time,
        false,
    )?.get_price();

    let pay_token_price = OraclePrice::new_from_oracle(
        pay_custody_oracle_account,
        current_time,
        false,
    )?.get_price();

    // Calculate time to expiration for CURRENT terms
    let current_time_to_expiry = math::checked_float_div(
        (option_detail.expired_date - current_time) as f64,
        365.25 * 24.0 * 3600.0 // seconds in a year
    )?;

    // Get utilization data for dynamic borrow rate
    let (token_locked, token_owned) = (locked_custody.token_locked, locked_custody.token_owned);
    let is_sol = custody.key() == locked_custody.key();

    // Get current size (convert from u64 quantity to f64 size)
    let current_size = option_detail.quantity as f64;

    // Calculate CURRENT total option value (old terms)
    let current_strike_f64 = scaled_price_to_f64(option_detail.strike_price)?;
    let current_option_value_per_unit = black_scholes_with_borrow_rate(
        underlying_price,
        current_strike_f64, // OLD strike converted to f64
        current_time_to_expiry,
        option_detail.option_type == 0,
        token_locked,
        token_owned,
        is_sol,
    )?;
    let current_total_option_value = current_option_value_per_unit * current_size;

    msg!("Current option value per unit: {}", current_option_value_per_unit);
    msg!("Current size: {}", current_size);
    msg!("Current total option value: {}", current_total_option_value);

    // Determine new parameters
    let new_strike = params.new_strike.unwrap_or(current_strike_f64);
    let new_expiry = params.new_expiry.unwrap_or(option_detail.expired_date);
    let new_size = params.new_size.unwrap_or(current_size);

    // Validate new parameters
    require!(new_size > 0.0, TradingError::InvalidParameterError);
    require!(new_strike > 0.0, TradingError::InvalidParameterError);
    require!(new_expiry > current_time, OptionError::InvalidTimeError);

    // Calculate time to expiration for NEW terms
    let new_time_to_expiry = math::checked_float_div(
        (new_expiry - current_time) as f64,
        365.25 * 24.0 * 3600.0
    )?;

    // Calculate NEW total option value (new terms)
    let new_option_value_per_unit = black_scholes_with_borrow_rate(
        underlying_price,
        new_strike, // NEW strike
        new_time_to_expiry,
        option_detail.option_type == 0,
        token_locked,
        token_owned,
        is_sol,
    )?;
    let new_total_option_value = new_option_value_per_unit * new_size;

    msg!("New option value per unit: {}", new_option_value_per_unit);
    msg!("New size: {}", new_size);
    msg!("New total option value: {}", new_total_option_value);

    // Calculate value difference for premium adjustment
    let value_difference = new_total_option_value - current_total_option_value;
    msg!("Value difference: {}", value_difference);

    // Handle premium adjustment based on value difference sign
    if value_difference > 0.0 {
        // User needs to pay MORE (new option is more valuable)
        
        // Convert to pay token amount (following open_option.rs pattern)
        let additional_premium = math::checked_as_u64(
            math::checked_float_div(value_difference, pay_token_price)?
                * math::checked_powi(10.0, pay_custody.decimals as i32)?
        )?;

        msg!("Additional premium needed: {}", additional_premium);
        
        // Slippage protection
        require_gte!(
            params.max_additional_premium,
            additional_premium,
            TradingError::SlippageExceededError
        );

        // Check user has enough balance
        require_gte!(
            funding_account.amount,
            additional_premium,
            TradingError::InvalidSignerBalanceError
        );

        // Transfer additional premium from user to pool
        token::transfer(
            CpiContext::new(
                token_program.to_account_info(),
                SplTransfer {
                    from: funding_account.to_account_info(),
                    to: pay_custody_token_account.to_account_info(),
                    authority: owner.to_account_info(),
                },
            ),
            additional_premium,
        )?;

        // Update pool balances
        pay_custody.token_owned = math::checked_add(pay_custody.token_owned, additional_premium)?;
        
        // Update option premium paid
        option_detail.premium = math::checked_add(option_detail.premium, additional_premium)?;

        msg!("User paid additional premium: {}", additional_premium);

    } else if value_difference < 0.0 {
        // User gets REFUNDED (new option is less valuable)
        let refund_amount = math::checked_as_u64(
            math::checked_float_div(-value_difference, pay_token_price)?
                * math::checked_powi(10.0, pay_custody.decimals as i32)?
        )?;

        msg!("Refund amount calculated: {}", refund_amount);
        
        // Apply platform fee (10% like in close_option.rs)
        let actual_refund = math::checked_div(math::checked_mul(refund_amount, 9)?, 10)?;

        // Slippage protection
        require_gte!(
            actual_refund,
            params.min_refund_amount,
            TradingError::SlippageExceededError
        );

        // Check pool has enough balance for refund
        require_gte!(
            math::checked_sub(pay_custody.token_owned, pay_custody.token_locked)?,
            actual_refund,
            PoolError::InvalidPoolBalanceError
        );

        // Transfer refund from pool to user
        contract.transfer_tokens(
            pay_custody_token_account.to_account_info(),
            refund_account.to_account_info(),
            transfer_authority.to_account_info(),
            token_program.to_account_info(),
            actual_refund,
        )?;

        // Update pool balances
        pay_custody.token_owned = math::checked_sub(pay_custody.token_owned, actual_refund)?;
        
        // Update option premium paid
        option_detail.premium = math::checked_sub(option_detail.premium, actual_refund)?;

        msg!("User received refund: {}", actual_refund);
    }
    // If value_difference == 0.0, no payment needed

    // Update option parameters
    option_detail.strike_price = f64_to_scaled_price(new_strike)?;
    option_detail.expired_date = new_expiry;

    option_detail.quantity = new_size as u64;
    
    option_detail.last_update_time = current_time;

    // Recalculate period in days (for consistency)
    let new_period_days = math::checked_div(
        new_expiry - option_detail.purchase_date as i64,
        86400 // seconds per day
    )? as u64;
    option_detail.period = new_period_days;

    msg!("Option edited successfully");
    msg!("New strike: {}", new_strike);
    msg!("New expiry: {}", new_expiry);
    msg!("New size: {}", new_size);

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: EditOptionParams)]
pub struct EditOption<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    // For additional premium payments (pay_custody asset)
    #[account(
        mut,
        constraint = funding_account.mint == pay_custody.mint,
        has_one = owner
    )]
    pub funding_account: Box<Account<'info, TokenAccount>>,

    // For refunds (pay_custody asset) - could be same as funding_account
    #[account(
        mut,
        constraint = refund_account.mint == pay_custody.mint,
        has_one = owner
    )]
    pub refund_account: Box<Account<'info, TokenAccount>>,

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
        seeds = [b"pool",
                 params.pool_name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        seeds = [b"user_v3", owner.key().as_ref()],
        bump,
    )]
    pub user: Box<Account<'info, User>>,

    // Mint accounts first (following close_option.rs pattern)
    #[account(mut)]
    pub custody_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub pay_custody_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub locked_custody_mint: Box<Account<'info, Mint>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody_mint.key().as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>, // underlying price asset

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 pay_custody_mint.key().as_ref()],
        bump = pay_custody.bump
    )]
    pub pay_custody: Box<Account<'info, Custody>>, // premium payment asset

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 locked_custody_mint.key().as_ref()],
        bump = locked_custody.bump
    )]
    pub locked_custody: Box<Account<'info, Custody>>, // locked asset

    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 pay_custody.mint.key().as_ref()],
        bump,
        constraint = pay_custody_token_account.mint == pay_custody_mint.key()
    )]
    pub pay_custody_token_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"option", owner.key().as_ref(),
            params.option_index.to_le_bytes().as_ref(),
            pool.key().as_ref(), custody.key().as_ref()],
        bump = option_detail.bump
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,

    /// CHECK: oracle for underlying asset
    #[account(constraint = custody_oracle_account.key() == custody.oracle)]
    pub custody_oracle_account: AccountInfo<'info>,

    /// CHECK: oracle for payment asset  
    #[account(constraint = pay_custody_oracle_account.key() == pay_custody.oracle)]
    pub pay_custody_oracle_account: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
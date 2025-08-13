use crate::{
    errors::{FutureError, TradingError},
    events::LimitFutureOpened,
    math::{self, f64_to_scaled_price},
    state::{Contract, Custody, Future, FutureStatus, OraclePrice, Pool, Side, User},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct OpenLimitFutureParams {
    pub size_usd: u64,                     // Position size in USD
    pub collateral_amount: u64,            // Collateral in tokens
    pub side: Side,                        // Long or Short
    pub expiry_timestamp: i64,             // Future expiry time
    pub trigger_price: u64,                // Execute when SOL hits this price
    pub trigger_above_threshold: bool,      // true = execute when price >= trigger, false = when price <= trigger
    pub max_slippage: u64,                 // Max acceptable slippage in basis points
    pub pool_name: String,                 // Pool name for seeds
    pub pay_sol: bool,                     // true = pay collateral in SOL, false = USDC
}

pub fn open_limit_future(
    ctx: Context<OpenLimitFuture>,
    params: &OpenLimitFutureParams,
) -> Result<()> {
    msg!("Opening limit future position");
    msg!("Trigger price: {}", params.trigger_price as f64 / 1_000_000.0);
    msg!("Size USD: {}", params.size_usd);

    // Get keys first to avoid borrowing conflicts
    let sol_custody_key = ctx.accounts.sol_custody.key();
    let usdc_custody_key = ctx.accounts.usdc_custody.key();
    
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let future = &mut ctx.accounts.future;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;

    // Get current time and validate expiry
    let current_time = contract.get_time()?;
    
    require!(
        params.expiry_timestamp > current_time,
        FutureError::InvalidExpiryTime
    );
    
    // Validate expiry is not too far in the future (max 1 year)
    let max_expiry = current_time + (365 * 24 * 60 * 60);
    require!(
        params.expiry_timestamp <= max_expiry,
        FutureError::ExpiryTooFar
    );

    // Get current prices for validation
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let current_sol_price_scaled = sol_price.scale_to_exponent(-6)?.price;

    // Validate trigger price is reasonable (within 50% of current price)
    let price_tolerance = current_sol_price_scaled / 2; // 50%
    require!(
        params.trigger_price >= current_sol_price_scaled.saturating_sub(price_tolerance) &&
        params.trigger_price <= current_sol_price_scaled.saturating_add(price_tolerance),
        TradingError::InvalidAmount
    );

    // Calculate collateral value in USD
    let collateral_price = if params.pay_sol { &sol_price } else { &usdc_price };
    let collateral_decimals = if params.pay_sol { sol_custody.decimals } else { usdc_custody.decimals };
    
    let collateral_usd = collateral_price.get_asset_amount_usd(
        params.collateral_amount,
        collateral_decimals
    )?;

    // Validate minimum collateral (at least 10% of position size)
    let min_collateral = params.size_usd / 10;
    require!(
        collateral_usd >= min_collateral,
        TradingError::InsufficientBalance
    );

    // Calculate time to expiry and fixed interest rate
    let time_to_expiry = params.expiry_timestamp - current_time;
    let fixed_rate_bps = pool.add_future_position(
        params.size_usd,
        time_to_expiry,
        current_time,
    )?;

    // Calculate future price using F = S * exp(r * t)
    let annual_rate = (fixed_rate_bps as f64) / 10_000.0;
    let time_fraction = (time_to_expiry as f64) / (365.25 * 24.0 * 3600.0);
    let future_price_f64 = (params.trigger_price as f64 / 1_000_000.0) * (annual_rate * time_fraction).exp();
    let future_price_scaled = f64_to_scaled_price(future_price_f64)?;

    // Calculate required liquidity to lock
    let locked_amount = if params.side == Side::Long {
        // Long positions lock SOL equivalent to position size
        let sol_price_scaled = sol_price.scale_to_exponent(-6)?;
        let sol_amount_6_decimals = math::checked_div(
            math::checked_mul(params.size_usd as u128, 1_000_000u128)?,
            sol_price_scaled.price as u128
        )?;
        
        if sol_custody.decimals > 6 {
            math::checked_as_u64(math::checked_mul(
                sol_amount_6_decimals,
                math::checked_pow(10u128, (sol_custody.decimals - 6) as usize)?
            )?)?
        } else {
            math::checked_as_u64(math::checked_div(
                sol_amount_6_decimals,
                math::checked_pow(10u128, (6 - sol_custody.decimals) as usize)?
            )?)?
        }
    } else {
        // Short positions lock USDC equivalent to position size
        let usdc_price_scaled = usdc_price.scale_to_exponent(-6)?;
        let usdc_amount_6_decimals = math::checked_div(
            math::checked_mul(params.size_usd as u128, 1_000_000u128)?,
            usdc_price_scaled.price as u128
        )?;
        
        if usdc_custody.decimals > 6 {
            math::checked_as_u64(math::checked_mul(
                usdc_amount_6_decimals,
                math::checked_pow(10u128, (usdc_custody.decimals - 6) as usize)?
            )?)?
        } else {
            math::checked_as_u64(math::checked_div(
                usdc_amount_6_decimals,
                math::checked_pow(10u128, (6 - usdc_custody.decimals) as usize)?
            )?)?
        }
    };

    // Check pool has sufficient liquidity (but don't lock it yet - only when executed)
    let available_liquidity = if params.side == Side::Long {
        math::checked_sub(sol_custody.token_owned, sol_custody.token_locked)?
    } else {
        math::checked_sub(usdc_custody.token_owned, usdc_custody.token_locked)?
    };
    
    require!(
        available_liquidity >= locked_amount,
        TradingError::InsufficientPoolLiquidity
    );

    // Transfer collateral from user to pool
    let collateral_token_account = if params.pay_sol {
        &ctx.accounts.sol_custody_token_account
    } else {
        &ctx.accounts.usdc_custody_token_account
    };

    contract.transfer_tokens(
        ctx.accounts.funding_account.to_account_info(),
        collateral_token_account.to_account_info(),
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        params.collateral_amount,
    )?;

    // Update collateral custody balance
    if params.pay_sol {
        sol_custody.token_owned = math::checked_add(
            sol_custody.token_owned,
            params.collateral_amount
        )?;
    } else {
        usdc_custody.token_owned = math::checked_add(
            usdc_custody.token_owned,
            params.collateral_amount
        )?;
    }

    // NOTE: We DON'T lock liquidity yet - only when the limit order executes

    // Initialize future position as PENDING (limit order)
    future.index = ctx.accounts.user.future_index;
    
    // Increment user's future index for next future
    ctx.accounts.user.future_index = math::checked_add(ctx.accounts.user.future_index, 1)?;
    
    future.owner = ctx.accounts.owner.key();
    future.pool = pool.key();
    future.custody = sol_custody_key; // Always SOL as underlying
    future.collateral_custody = if params.pay_sol {
        sol_custody_key
    } else {
        usdc_custody_key
    };
    
    future.side = params.side;
    future.status = FutureStatus::Pending; // Limit order waiting execution
    
    // Use trigger price as the "spot price at open" since that's when it will execute
    future.entry_price = params.trigger_price;
    future.future_price = future_price_scaled;
    future.size_usd = params.size_usd;
    future.collateral_usd = collateral_usd;
    future.collateral_amount = params.collateral_amount;
    
    future.open_time = current_time;
    future.expiry_time = params.expiry_timestamp;
    future.update_time = current_time;

    // Limit order specific fields
    future.trigger_price = Some(params.trigger_price);
    future.trigger_above_threshold = params.trigger_above_threshold;
    future.max_slippage = params.max_slippage;
    future.execution_time = None; // Will be set when executed
    
    future.fixed_interest_rate_bps = fixed_rate_bps;
    future.liquidation_price = 0; // Will be calculated when executed
    
    future.pnl_at_settlement = None;
    future.settlement_price = None;
    future.settlement_amount = None;
    
    future.opening_fee = 0; // No fees for limit orders until execution
    future.settlement_fee = 0;
    
    future.locked_amount = locked_amount; // Store for future use
    future.bump = ctx.bumps.future;

    emit!(LimitFutureOpened {
        owner: future.owner,
        future_key: future.key(),
        index: future.index,
        pool: pool.key(),
        custody: sol_custody_key,
        collateral_custody: future.collateral_custody,
        side: future.side as u8,
        size_usd: params.size_usd,
        collateral_usd,
        collateral_amount: params.collateral_amount,
        trigger_price: params.trigger_price,
        trigger_above_threshold: params.trigger_above_threshold,
        future_price: future_price_scaled,
        fixed_interest_rate_bps: fixed_rate_bps,
        expiry_time: params.expiry_timestamp,
        max_slippage: params.max_slippage,
        open_time: current_time,
    });

    msg!("Limit future position created successfully");
    msg!("Trigger price: {}", params.trigger_price);
    msg!("Future price: {}", future_price_scaled);
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: OpenLimitFutureParams)]
pub struct OpenLimitFuture<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        init_if_needed,
        payer = owner,
        space = User::LEN,
        seeds = [b"user_v3", owner.key().as_ref()],
        bump
    )]
    pub user: Box<Account<'info, User>>,

    #[account(mut)]
    pub funding_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: Transfer authority PDA for contract token operations
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

    #[account(
        init,
        payer = owner,
        space = Future::LEN,
        seeds = [
            b"future",
            owner.key().as_ref(),
            user.future_index.to_le_bytes().as_ref(),
            pool.key().as_ref()
        ],
        bump
    )]
    pub future: Box<Account<'info, Future>>,

    #[account(
        mut,
        seeds = [b"custody", pool.key().as_ref(), sol_mint.key().as_ref()],
        bump = sol_custody.bump
    )]
    pub sol_custody: Box<Account<'info, Custody>>,

    #[account(
        mut,
        seeds = [b"custody", pool.key().as_ref(), usdc_mint.key().as_ref()],
        bump = usdc_custody.bump
    )]
    pub usdc_custody: Box<Account<'info, Custody>>,

    /// CHECK: Oracle account validation is handled by constraint
    #[account(
        constraint = sol_oracle_account.key() == sol_custody.oracle
    )]
    pub sol_oracle_account: AccountInfo<'info>,

    /// CHECK: Oracle account validation is handled by constraint
    #[account(
        constraint = usdc_oracle_account.key() == usdc_custody.oracle
    )]
    pub usdc_oracle_account: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [
            b"custody_token_account",
            pool.key().as_ref(),
            sol_custody.mint.key().as_ref()
        ],
        bump
    )]
    pub sol_custody_token_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [
            b"custody_token_account",
            pool.key().as_ref(),
            usdc_custody.mint.key().as_ref()
        ],
        bump
    )]
    pub usdc_custody_token_account: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub sol_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub usdc_mint: Box<Account<'info, Mint>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
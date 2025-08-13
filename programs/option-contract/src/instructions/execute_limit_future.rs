use crate::{
    errors::{FutureError, TradingError},
    events::LimitFutureExecuted,
    math::{self, f64_to_scaled_price},
    state::{Contract, Custody, Future, FutureStatus, OraclePrice, Pool, Side},
    utils::risk_management::*,
};
use anchor_lang::prelude::*;
use anchor_spl::token::Mint;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ExecuteLimitFutureParams {
    pub future_index: u64,
    pub pool_name: String,
    pub owner: Pubkey,             // Owner of the future (for seeds)
    pub execution_price: u64,      // Actual execution price (should be close to trigger)
}

pub fn execute_limit_future(
    ctx: Context<ExecuteLimitFuture>,
    params: &ExecuteLimitFutureParams,
) -> Result<()> {
    msg!("Executing limit future order");
    msg!("Execution price: {}", params.execution_price as f64 / 1_000_000.0);

    // Get keys first to avoid borrowing conflicts
    let sol_custody_key = ctx.accounts.sol_custody.key();
    
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let future = &mut ctx.accounts.future;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;

    // Validation
    require_keys_eq!(
        future.owner,
        params.owner,
        TradingError::Unauthorized
    );

    require!(
        future.status == FutureStatus::Pending,
        FutureError::FutureNotPending
    );

    let current_time = contract.get_time()?;

    // Check if future has already expired
    require!(
        !future.is_expired(current_time),
        FutureError::FutureExpired
    );

    // Get current price and validate trigger condition
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let current_price_scaled = sol_price.scale_to_exponent(-6)?.price;

    let trigger_price = future.trigger_price.ok_or(FutureError::NoTriggerPrice)?;

    // Check if trigger condition is met
    let trigger_met = if future.trigger_above_threshold {
        current_price_scaled >= trigger_price
    } else {
        current_price_scaled <= trigger_price
    };

    require!(trigger_met, FutureError::TriggerConditionNotMet);

    // Validate execution price is within slippage tolerance
    let price_diff = if params.execution_price > trigger_price {
        params.execution_price - trigger_price
    } else {
        trigger_price - params.execution_price
    };

    let max_slippage_amount = math::checked_div(
        math::checked_mul(trigger_price as u128, future.max_slippage as u128)?,
        10_000u128
    )?;

    require!(
        price_diff as u128 <= max_slippage_amount,
        TradingError::SlippageExceededError
    );

    // Now execute the future at the execution price
    future.entry_price = params.execution_price;
    future.status = FutureStatus::Active;
    future.execution_time = Some(current_time);
    future.update_time = current_time;

    // Recalculate future price based on actual execution price
    let time_to_expiry = future.expiry_time - current_time;
    let annual_rate = (future.fixed_interest_rate_bps as f64) / 10_000.0;
    let time_fraction = (time_to_expiry as f64) / (365.25 * 24.0 * 3600.0);
    let future_price_f64 = (params.execution_price as f64 / 1_000_000.0) * (annual_rate * time_fraction).exp();
    future.future_price = f64_to_scaled_price(future_price_f64)?;

    // Calculate liquidation price
    let leverage = (future.size_usd as f64) / (future.collateral_usd as f64);
    future.liquidation_price = calculate_liquidation_price(
        params.execution_price,
        leverage,
        future.side
    )?;

    // Now lock the required liquidity in the pool
    if future.side == Side::Long {
        sol_custody.token_locked = math::checked_add(
            sol_custody.token_locked,
            future.locked_amount
        )?;
    } else {
        usdc_custody.token_locked = math::checked_add(
            usdc_custody.token_locked,
            future.locked_amount
        )?;
    }

    // Check if we still have sufficient liquidity
    let _available_liquidity = if future.side == Side::Long {
        math::checked_sub(sol_custody.token_owned, sol_custody.token_locked)?
    } else {
        math::checked_sub(usdc_custody.token_owned, usdc_custody.token_locked)?
    };

    require!(
        sol_custody.token_locked <= sol_custody.token_owned &&
        usdc_custody.token_locked <= usdc_custody.token_owned,
        TradingError::InsufficientPoolLiquidity
    );

    emit!(LimitFutureExecuted {
        owner: future.owner,
        future_key: future.key(),
        index: future.index,
        pool: pool.key(),
        custody: sol_custody_key,
        collateral_custody: future.collateral_custody,
        side: future.side as u8,
        size_usd: future.size_usd,
        trigger_price,
        execution_price: params.execution_price,
        future_price: future.future_price,
        liquidation_price: future.liquidation_price,
        execution_time: current_time,
        expiry_time: future.expiry_time,
        locked_amount: future.locked_amount,
    });

    msg!("Limit future order executed successfully");
    msg!("New future price: {}", future.future_price);
    msg!("Liquidation price: {}", future.liquidation_price);

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: ExecuteLimitFutureParams)]
pub struct ExecuteLimitFuture<'info> {
    /// CHECK: This can be any account - keeper, owner, or other authorized party
    #[account(mut)]
    pub executor: Signer<'info>,

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
        mut,
        seeds = [
            b"future",
            params.owner.as_ref(),
            params.future_index.to_le_bytes().as_ref(),
            pool.key().as_ref()
        ],
        bump = future.bump
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

    #[account(mut)]
    pub sol_mint: Box<Account<'info, Mint>>,
    
    #[account(mut)]
    pub usdc_mint: Box<Account<'info, Mint>>,
}
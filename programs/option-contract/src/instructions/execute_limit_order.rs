use crate::{
    errors::{PerpetualError, TradingError},
    events::LimitOrderExecuted,
    math::{self, f64_to_scaled_price},
    utils::risk_management::*,
    state::{Contract, Custody, OraclePrice, Pool, Position, PositionType, Side, User},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ExecuteLimitOrderParams {
    pub position_index: u64,
    pub pool_name: String,
    pub execution_price: f64,  // Actual execution price
}

pub fn execute_limit_order(
    ctx: Context<ExecuteLimitOrder>,
    params: &ExecuteLimitOrderParams
) -> Result<()> {
    msg!("Executing limit order");
    
    let contract = &ctx.accounts.contract;
    let position = &mut ctx.accounts.position;
    let pool = &mut ctx.accounts.pool;
    let _sol_custody = &mut ctx.accounts.sol_custody;
    let _usdc_custody = &mut ctx.accounts.usdc_custody;
    
    // Validation
    require_keys_eq!(position.owner, ctx.accounts.owner.key(), TradingError::Unauthorized);
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require!(position.position_type == PositionType::Limit, PerpetualError::NotLimitOrder);
    require!(params.execution_price > 0.0, TradingError::InvalidPrice);
    
    // Get current time and prices
    let current_time = contract.get_time()?;
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let current_sol_price = sol_price.get_price();
    let _usdc_price_value = usdc_price.get_price();
    let execution_price_scaled = f64_to_scaled_price(params.execution_price)?;
    
    msg!("Current SOL price: {}", current_sol_price);
    msg!("Execution price: {}", params.execution_price);
    msg!("Position side: {:?}", position.side);
    msg!("Position trigger price: {:?}", position.trigger_price);
    
    // Validate that the limit order should be executed at this price
    require!(
        position.should_execute_limit_order(execution_price_scaled),
        PerpetualError::LimitOrderNotTriggered
    );
    
    // Additional validation: ensure execution price is reasonable based on current market price
    let current_price_scaled = f64_to_scaled_price(current_sol_price)?;
    let price_tolerance = math::checked_div(current_price_scaled, 100)?; // 1% tolerance
    
    require!(
        execution_price_scaled >= current_price_scaled.saturating_sub(price_tolerance) &&
        execution_price_scaled <= current_price_scaled.saturating_add(price_tolerance),
        PerpetualError::InvalidExecutionPrice
    );

    let new_leverage = math::checked_div(position.size_usd, position.collateral_usd)?;
    
    // Calculate liquidation price for the new market position
    let liquidation_price = calculate_liquidation_price(
        execution_price_scaled,
        new_leverage,
        position.side
    )?;
    
    // Get current cumulative funding and interest snapshots from pool
    // These will be set when the position becomes a market position
    let _current_cumulative_funding = if position.side == Side::Long {
        pool.cumulative_funding_rate_long
    } else {
        pool.cumulative_funding_rate_short
    };
    let current_cumulative_interest = pool.cumulative_interest_rate;
    
    // Execute the limit order (convert to market position)
    position.execute_limit_order(execution_price_scaled, current_time)?;
    
    // Update position with market position specifics
    position.liquidation_price = liquidation_price;
    
    // Set funding and interest snapshots to current values (start tracking from execution)
    // Limit orders don't pay funding/interest until they become market positions
    // No funding snapshot update needed in peer-to-pool model;
    position.cumulative_interest_snapshot = current_cumulative_interest;
    
    // Update pool open interest tracking
    if position.side == Side::Long {
        pool.long_open_interest_usd = math::checked_add(
            pool.long_open_interest_usd,
            position.size_usd as u128
        )?;
    } else {
        pool.short_open_interest_usd = math::checked_add(
            pool.short_open_interest_usd,
            position.size_usd as u128
        )?;
    }
    
    emit!(LimitOrderExecuted {
        owner: position.owner,
        pool: position.pool,
        custody: position.custody,
        collateral_custody: position.collateral_custody,
        position_type: position.position_type as u8,
        side: position.side as u8,
        is_liquidated: position.is_liquidated,
        price: position.price,
        size_usd: position.size_usd,
        borrow_size_usd: position.borrow_size_usd,
        collateral_usd: position.collateral_usd,
        open_time: position.open_time,
        update_time: position.update_time,
        liquidation_price: position.liquidation_price,
        cumulative_interest_snapshot: position.cumulative_interest_snapshot,
        opening_fee_paid: position.opening_fee_paid,
        total_fees_paid: position.total_fees_paid,
        locked_amount: position.locked_amount,
        collateral_amount: position.collateral_amount,
        take_profit_price: position.take_profit_price,
        stop_loss_price: position.stop_loss_price,
        trigger_price: position.trigger_price,
        trigger_above_threshold: position.trigger_above_threshold,
        bump: position.bump,
        execution_price: execution_price_scaled,
    });
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: ExecuteLimitOrderParams)]
pub struct ExecuteLimitOrder<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

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
        seeds = [b"user", owner.key().as_ref()],
        bump = user.bump
    )]
    pub user: Box<Account<'info, User>>,

    #[account(
        mut,
        seeds = [
            b"position",
            owner.key().as_ref(),
            params.position_index.to_le_bytes().as_ref(),
            pool.key().as_ref()
        ],
        bump = position.bump
    )]
    pub position: Box<Account<'info, Position>>,

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

    #[account(mut)]
    pub sol_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub usdc_mint: Box<Account<'info, Mint>>,

    pub token_program: Program<'info, Token>,
}
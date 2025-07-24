use crate::{
    errors::PerpetualError,
    events::BorrowFeesUpdated,
    state::{Contract, Custody, Pool, Position, OrderType},
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UpdateBorrowFeesParams {
    pub position_index: u64,
    pub pool_name: String,
}

pub fn update_borrow_fees(
    ctx: Context<UpdateBorrowFees>,
    params: &UpdateBorrowFeesParams
) -> Result<()> {
    msg!("Updating borrow fees for position");
    
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    let position = &mut ctx.accounts.position;
    
    // Validation
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require!(position.order_type == OrderType::Market, PerpetualError::InvalidOrderType);
    
    let current_time = contract.get_time()?;
    
    // Store previous values for logging
    let previous_interest_snapshot = position.cumulative_interest_snapshot;
    let previous_borrow_fee_update_time = position.last_borrow_fees_update_time;
    
    // Calculate time-based borrow fee accrual using the helper method
    let borrow_fee_payment = pool.update_position_borrow_fees(
        position, 
        current_time, 
        sol_custody, 
        usdc_custody
    )?;
    
    // Get relevant custody for logging
    let relevant_custody = match position.side {
        crate::state::Side::Long => sol_custody.as_ref(),  // Long positions borrow SOL
        crate::state::Side::Short => usdc_custody.as_ref(), // Short positions borrow USDC
    };
    let current_borrow_rate = pool.get_token_borrow_rate(relevant_custody)?;
    let current_borrow_rate_bps = current_borrow_rate.to_bps().unwrap_or(0u32);
    
    msg!("Position size USD: {}", position.size_usd);
    msg!("Position side: {:?}", position.side);
    msg!("Using custody utilization: {:.2}%", crate::utils::pool::calculate_utilization(relevant_custody.token_locked, relevant_custody.token_owned));
    msg!("Current borrow rate: {:.2}% APR", current_borrow_rate_bps as f64 / 100.0);
    msg!("Time elapsed: {} seconds", current_time - previous_borrow_fee_update_time);
    msg!("Borrow fee payment: {}", borrow_fee_payment);
    
    msg!("Updated accrued borrow fees: {}", position.accrued_borrow_fees);
    msg!("New interest snapshot: {}", position.cumulative_interest_snapshot);
    
    emit!(BorrowFeesUpdated {
        pub_key: position.key(),
        owner: position.owner,
        position_index: params.position_index,
        pool: pool.key(),
        custody: position.custody,
        collateral_custody: position.collateral_custody,
        order_type: position.order_type as u8,
        side: position.side as u8,
        position_size_usd: position.size_usd,
        borrow_fee_payment: borrow_fee_payment.try_into().unwrap(),
        new_accrued_borrow_fees: position.accrued_borrow_fees,
        last_borrow_fees_update_time: position.last_borrow_fees_update_time,
        previous_interest_snapshot,
        new_interest_snapshot: current_borrow_rate_bps as u128,
        update_time: current_time,
    });
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: UpdateBorrowFeesParams)]
pub struct UpdateBorrowFees<'info> {
    /// CHECK: This can be any account, typically a keeper bot
    pub keeper: Signer<'info>,

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
            b"position",
            position.owner.as_ref(),
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

    #[account(mut)]
    pub sol_mint: Box<Account<'info, anchor_spl::token::Mint>>,
    #[account(mut)]
    pub usdc_mint: Box<Account<'info, anchor_spl::token::Mint>>,
}
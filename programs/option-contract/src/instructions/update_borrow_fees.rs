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
    
    // Get current time and update pool rates
    let current_time = contract.get_time()?;
    let custodies_slice = [sol_custody.as_ref(), usdc_custody.as_ref()];
    let custodies_vec: Vec<Custody> = custodies_slice.iter().map(|c| (***c).clone()).collect();
    pool.update_rates(current_time, &custodies_vec)?;
    
    // Store previous snapshot
    let previous_interest_snapshot = position.cumulative_interest_snapshot;
    
    // Calculate borrow fee payment (on borrowed funds, side-specific)
    let borrow_fee_payment = pool.get_interest_payment(
        position.borrow_size_usd as u128,
        position.cumulative_interest_snapshot,
        position.side
    )?;
    
    msg!("Position size USD: {}", position.size_usd);
    msg!("Borrow size USD: {}", position.borrow_size_usd);
    msg!("Borrow fee payment: {}", borrow_fee_payment);
    
    // Get new snapshot (side-specific)
    let new_interest_snapshot = match position.side {
        crate::state::Side::Long => pool.cumulative_interest_rate_long,
        crate::state::Side::Short => pool.cumulative_interest_rate_short,
    };
    
    // Update position with accrued borrow fees
    position.update_accrued_borrow_fees(
        borrow_fee_payment.try_into().unwrap(),
        new_interest_snapshot,
        current_time,
    )?;
    
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
        borrow_size_usd: position.borrow_size_usd,
        borrow_fee_payment: borrow_fee_payment.try_into().unwrap(),
        new_accrued_borrow_fees: position.accrued_borrow_fees,
        previous_interest_snapshot,
        new_interest_snapshot,
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
use crate::{
    errors::{PerpetualError, TradingError},
    math,
    state::{Contract, Custody, Pool, Position, PositionType, Side, User},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CancelLimitOrderParams {
    pub position_index: u64,
    pub pool_name: String,
    pub receive_sol: bool,  // true = receive SOL, false = receive USDC
}

pub fn cancel_limit_order(
    ctx: Context<CancelLimitOrder>,
    params: &CancelLimitOrderParams
) -> Result<()> {
    msg!("Canceling limit order");
    
    let contract = &ctx.accounts.contract;
    let position = &mut ctx.accounts.position;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    
    // Validation
    require_keys_eq!(position.owner, ctx.accounts.owner.key(), TradingError::Unauthorized);
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require!(position.position_type == PositionType::Limit, PerpetualError::NotLimitOrder);
    require!(position.size_usd > 0, PerpetualError::InvalidPositionSize);
    
    let current_time = contract.get_time()?;
    
    msg!("Canceling limit order for position:");
    msg!("Position size USD: {}", position.size_usd);
    msg!("Collateral USD: {}", position.collateral_usd);
    msg!("Collateral amount: {}", position.collateral_amount);
    msg!("Position side: {:?}", position.side);
    msg!("User chose to receive: {}", if params.receive_sol { "SOL" } else { "USDC" });
    
    // Calculate refund amounts
    let collateral_to_refund = position.collateral_amount;
    let collateral_usd_to_refund = position.collateral_usd;
    
    // Determine refund asset and calculate refund amount
    let (refund_custody, refund_token_account) = if params.receive_sol {
        (sol_custody, &ctx.accounts.sol_custody_token_account)
    } else {
        (usdc_custody, &ctx.accounts.usdc_custody_token_account)
    };
    
    // Transfer collateral back to user
    if collateral_to_refund > 0 {
        // Use the same collateral asset that was originally deposited
        let original_collateral_custody = if position.collateral_custody == sol_custody.key() {
            sol_custody
        } else {
            usdc_custody
        };
        
        let original_token_account = if position.collateral_custody == sol_custody.key() {
            &ctx.accounts.sol_custody_token_account
        } else {
            &ctx.accounts.usdc_custody_token_account
        };
        
        // Transfer original collateral back to user
        ctx.accounts.contract.transfer_tokens(
            original_token_account.to_account_info(),
            ctx.accounts.receiving_account.to_account_info(),
            ctx.accounts.transfer_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            collateral_to_refund,
        )?;
        
        // Update custody stats - remove collateral from pool
        if position.collateral_custody == sol_custody.key() {
            sol_custody.token_owned = math::checked_sub(
                sol_custody.token_owned,
                collateral_to_refund
            )?;
        } else {
            usdc_custody.token_owned = math::checked_sub(
                usdc_custody.token_owned,
                collateral_to_refund
            )?;
        }
    }
    
    // Release locked liquidity from the position
    if position.locked_amount > 0 {
        if position.side == Side::Long {
            sol_custody.token_locked = math::checked_sub(
                sol_custody.token_locked,
                position.locked_amount
            )?;
        } else {
            usdc_custody.token_locked = math::checked_sub(
                usdc_custody.token_locked,
                position.locked_amount
            )?;
        }
    }
    
    // Since this was a limit order, no funding or interest was paid
    // (limit orders don't pay funding/interest until executed)
    
    // Mark position as liquidated (canceled)
    position.is_liquidated = true;
    position.size_usd = 0;
    position.collateral_amount = 0;
    position.collateral_usd = 0;
    position.locked_amount = 0;
    position.trigger_price = None;
    position.position_type = PositionType::Market; // Reset to market for cleanup
    position.update_time = current_time;
    
    // No fees for canceling limit orders since they were never active positions
    
    msg!("Successfully canceled limit order");
    msg!("Refunded collateral: {} tokens", collateral_to_refund);
    msg!("Refunded collateral USD: {}", collateral_usd_to_refund);
    msg!("Released locked liquidity: {}", position.locked_amount);
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: CancelLimitOrderParams)]
pub struct CancelLimitOrder<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = owner
    )]
    pub receiving_account: Box<Account<'info, TokenAccount>>,

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
}
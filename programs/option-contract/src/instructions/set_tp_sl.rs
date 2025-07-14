use crate::{
    errors::TradingError,
    state::{Contract, PerpPosition, User},
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SetTpSlParams {
    pub take_profit_price: Option<f64>,  // Optional take profit price
    pub stop_loss_price: Option<f64>,    // Optional stop loss price
}

pub fn set_tp_sl(ctx: Context<SetTpSl>, params: &SetTpSlParams) -> Result<()> {
    msg!("Setting TP/SL for perpetual position");
    
    let position = &mut ctx.accounts.position;
    let owner = &ctx.accounts.owner;
    
    // Verify ownership
    require!(position.owner == owner.key(), TradingError::Unauthorized);
    require!(!position.is_liquidated, TradingError::InvalidParameterError);
    
    // Set TP/SL using the position's method
    position.set_tp_sl(params.take_profit_price, params.stop_loss_price)?;
    
    msg!("TP/SL updated successfully");
    msg!("Take Profit: {:?}", params.take_profit_price);
    msg!("Stop Loss: {:?}", params.stop_loss_price);
    
    Ok(())
}

#[derive(Accounts)]
pub struct SetTpSl<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    
    #[account(
        mut,
        seeds = [b"position", owner.key().as_ref()],
        bump = position.bump
    )]
    pub position: Account<'info, PerpPosition>,
    
    pub contract: Account<'info, Contract>,
    
    #[account(mut)]
    pub user: Account<'info, User>,
    
    pub system_program: Program<'info, System>,
}
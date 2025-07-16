use crate::{
    errors::{PerpetualError, TradingError},
    events::PerpTpSlSet,
    state::{Pool, Position, Side, PositionType},
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SetTpSlParams {
    pub position_index: u64,
    pub pool_name: String,
    pub take_profit_price: Option<u64>,  // TP price (scaled)
    pub stop_loss_price: Option<u64>,    // SL price (scaled)
}

pub fn set_tp_sl(
    ctx: Context<SetTpSl>,
    params: &SetTpSlParams
) -> Result<()> {
    msg!("Setting TP/SL for perpetual position");
    
    let position = &mut ctx.accounts.position;
    
    // Validation
    require_keys_eq!(position.owner, ctx.accounts.owner.key(), TradingError::Unauthorized);
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require!(position.position_type == PositionType::Market, PerpetualError::InvalidPositionType);
    
    // Validate TP/SL prices against entry price
    if let Some(tp_price) = params.take_profit_price {
        match position.side {
            Side::Long => {
                require!(tp_price > position.price, TradingError::InvalidTakeProfitPrice);
            },
            Side::Short => {
                require!(tp_price < position.price, TradingError::InvalidTakeProfitPrice);
            }
        }
    }
    
    if let Some(sl_price) = params.stop_loss_price {
        match position.side {
            Side::Long => {
                require!(sl_price < position.price, TradingError::InvalidStopLossPrice);
            },
            Side::Short => {
                require!(sl_price > position.price, TradingError::InvalidStopLossPrice);
            }
        }
    }
    
    // Validate that SL is not beyond liquidation price
    if let Some(sl_price) = params.stop_loss_price {
        match position.side {
            Side::Long => {
                require!(
                    sl_price > position.liquidation_price,
                    TradingError::InvalidStopLossPrice
                );
            },
            Side::Short => {
                require!(
                    sl_price < position.liquidation_price,
                    TradingError::InvalidStopLossPrice
                );
            }
        }
    }
    
    // Update TP/SL
    position.update_tp_sl(params.take_profit_price, params.stop_loss_price)?;
    
    emit!(PerpTpSlSet {
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
        cumulative_funding_snapshot: position.cumulative_funding_snapshot,
        opening_fee_paid: position.opening_fee_paid,
        total_fees_paid: position.total_fees_paid,
        locked_amount: position.locked_amount,
        collateral_amount: position.collateral_amount,
        take_profit_price: position.take_profit_price,
        stop_loss_price: position.stop_loss_price,
        trigger_price: position.trigger_price,
        trigger_above_threshold: position.trigger_above_threshold,
        bump: position.bump,
    });
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: SetTpSlParams)]
pub struct SetTpSl<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

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
            owner.key().as_ref(),
            params.position_index.to_le_bytes().as_ref(),
            pool.key().as_ref()
        ],
        bump = position.bump
    )]
    pub position: Box<Account<'info, Position>>,
}
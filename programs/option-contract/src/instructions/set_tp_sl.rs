use crate::{
    errors::{PerpetualError, TradingError},
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
    
    msg!("Successfully set TP/SL");
    msg!("Take Profit Price: {:?}", position.take_profit_price);
    msg!("Stop Loss Price: {:?}", position.stop_loss_price);
    msg!("Entry Price: {}", position.price);
    msg!("Liquidation Price: {}", position.liquidation_price);
    msg!("Position Side: {:?}", position.side);
    
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
use crate::{
    errors::{TradingError, PerpetualError, OptionError},
    events::{TpSlOrderAdded, TpSlOrderRemoved, TpSlOrderUpdated},
    state::{Pool, Position, OptionDetail, TpSlOrderbook, Side},
    math::scaled_price_to_f64,
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum OrderAction {
    AddTakeProfit { price: u64, size_percent: u16 },
    AddStopLoss { price: u64, size_percent: u16 },
    UpdateTakeProfit { index: u8, new_price: Option<u64>, new_size_percent: Option<u16> },
    UpdateStopLoss { index: u8, new_price: Option<u64>, new_size_percent: Option<u16> },
    RemoveTakeProfit { index: u8 },
    RemoveStopLoss { index: u8 },
    ClearAll,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ManageTpSlOrdersParams {
    pub position_type: u8,
    pub position_index: u64,
    pub pool_name: String,
    pub action: OrderAction,
}

pub fn manage_tp_sl_orders(
    ctx: Context<ManageTpSlOrders>,
    params: &ManageTpSlOrdersParams
) -> Result<()> {
    msg!("Managing TP/SL orders");
    
    let orderbook = &mut ctx.accounts.tp_sl_orderbook;
    let owner = ctx.accounts.owner.key();
    
    // Validation
    require_keys_eq!(orderbook.owner, owner, TradingError::Unauthorized);
    require_eq!(orderbook.position_type, params.position_type, TradingError::InvalidPositionType);
    
    // Additional validation based on position type
    match params.position_type {
        0 => {
            // Perp position validation
            let position = ctx.accounts.position.as_ref().unwrap();
            require_keys_eq!(position.owner, owner, TradingError::Unauthorized);
            require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
            require_keys_eq!(orderbook.position, position.key(), TradingError::InvalidPosition);
            
            // Validate prices based on position side
            match &params.action {
                OrderAction::AddTakeProfit { price, .. } | 
                OrderAction::UpdateTakeProfit { new_price: Some(price), .. } => {
                    match position.side {
                        Side::Long => require!(*price > position.price, TradingError::InvalidTakeProfitPrice),
                        Side::Short => require!(*price < position.price, TradingError::InvalidTakeProfitPrice),
                    }
                },
                OrderAction::AddStopLoss { price, .. } |
                OrderAction::UpdateStopLoss { new_price: Some(price), .. } => {
                    match position.side {
                        Side::Long => {
                            require!(*price < position.price, TradingError::InvalidStopLossPrice);
                            require!(*price > position.liquidation_price, TradingError::InvalidStopLossPrice);
                        },
                        Side::Short => {
                            require!(*price > position.price, TradingError::InvalidStopLossPrice);
                            require!(*price < position.liquidation_price, TradingError::InvalidStopLossPrice);
                        }
                    }
                },
                _ => {}
            }
        },
        1 => {
            // Option position validation
            let option = ctx.accounts.option_detail.as_ref().unwrap();
            require_keys_eq!(option.owner, owner, TradingError::Unauthorized);
            require!(option.valid, OptionError::InvalidOption);
            require!(!option.executed, OptionError::OptionExecuted);
            require_keys_eq!(orderbook.position, option.key(), TradingError::InvalidPosition);
            
            // Validate prices based on option type
            let strike_price_f64 = scaled_price_to_f64(option.strike_price)?;
            match &params.action {
                OrderAction::AddTakeProfit { price, .. } |
                OrderAction::UpdateTakeProfit { new_price: Some(price), .. } => {
                    let price_f64 = scaled_price_to_f64(*price)?;
                    if option.option_type == 0 { // Call
                        require!(price_f64 > strike_price_f64, TradingError::InvalidTakeProfitPrice);
                    } else { // Put
                        require!(price_f64 < strike_price_f64, TradingError::InvalidTakeProfitPrice);
                    }
                },
                OrderAction::AddStopLoss { price, .. } |
                OrderAction::UpdateStopLoss { new_price: Some(price), .. } => {
                    let price_f64 = scaled_price_to_f64(*price)?;
                    if option.option_type == 0 { // Call
                        require!(price_f64 < strike_price_f64, TradingError::InvalidStopLossPrice);
                    } else { // Put
                        require!(price_f64 > strike_price_f64, TradingError::InvalidStopLossPrice);
                    }
                },
                _ => {}
            }
        },
        _ => return Err(TradingError::InvalidPositionType.into()),
    }
    
    // Execute action
    match params.action {
        OrderAction::AddTakeProfit { price, size_percent } => {
            let index = orderbook.add_take_profit_order(price, size_percent)?;
            emit!(TpSlOrderAdded {
                owner,
                position: orderbook.position,
                position_type: orderbook.position_type,
                order_type: 0, // 0 = TP
                index: index as u8,
                price,
                size_percent,
            });
        },
        OrderAction::AddStopLoss { price, size_percent } => {
            let index = orderbook.add_stop_loss_order(price, size_percent)?;
            emit!(TpSlOrderAdded {
                owner,
                position: orderbook.position,
                position_type: orderbook.position_type,
                order_type: 1, // 1 = SL
                index: index as u8,
                price,
                size_percent,
            });
        },
        OrderAction::UpdateTakeProfit { index, new_price, new_size_percent } => {
            orderbook.update_take_profit_order(index as usize, new_price, new_size_percent)?;
            emit!(TpSlOrderUpdated {
                owner,
                position: orderbook.position,
                position_type: orderbook.position_type,
                order_type: 0, // 0 = TP
                index,
                new_price,
                new_size_percent,
            });
        },
        OrderAction::UpdateStopLoss { index, new_price, new_size_percent } => {
            orderbook.update_stop_loss_order(index as usize, new_price, new_size_percent)?;
            emit!(TpSlOrderUpdated {
                owner,
                position: orderbook.position,
                position_type: orderbook.position_type,
                order_type: 1, // 1 = SL
                index,
                new_price,
                new_size_percent,
            });
        },
        OrderAction::RemoveTakeProfit { index } => {
            orderbook.remove_take_profit_order(index as usize)?;
            emit!(TpSlOrderRemoved {
                owner,
                position: orderbook.position,
                position_type: orderbook.position_type,
                order_type: 0, // 0 = TP
                index,
            });
        },
        OrderAction::RemoveStopLoss { index } => {
            orderbook.remove_stop_loss_order(index as usize)?;
            emit!(TpSlOrderRemoved {
                owner,
                position: orderbook.position,
                position_type: orderbook.position_type,
                order_type: 1, // 1 = SL
                index,
            });
        },
        OrderAction::ClearAll => {
            orderbook.clear_all_orders()?;
            msg!("All TP/SL orders cleared");
        }
    }
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: ManageTpSlOrdersParams)]
pub struct ManageTpSlOrders<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    
    #[account(
        mut,
        seeds = [
            b"tp_sl_orderbook",
            owner.key().as_ref(),
            params.position_index.to_le_bytes().as_ref(),
            params.pool_name.as_bytes(),
            params.position_type.to_le_bytes().as_ref(),
        ],
        bump = tp_sl_orderbook.bump
    )]
    pub tp_sl_orderbook: Box<Account<'info, TpSlOrderbook>>,
    
    #[account(
        seeds = [b"pool", params.pool_name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,
    
    // Position account (for perps - only present when position_type = 0)
    pub position: Option<Box<Account<'info, Position>>>,
    
    // Option account (for options - only present when position_type = 1)
    pub option_detail: Option<Box<Account<'info, OptionDetail>>>,
}
use crate::{
    errors::{TradingError, PerpetualError, OptionError},
    events::{TpSlOrderAdded, TpSlOrderRemoved, TpSlOrderUpdated},
    state::{Pool, Position, OptionDetail, TpSlOrderbook, Side, Contract, Custody},
    math::scaled_price_to_f64,
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum OrderAction {
    AddTakeProfit { price: u64, size_percent: u64, receive_sol: bool },
    AddStopLoss { price: u64, size_percent: u64, receive_sol: bool },
    UpdateTakeProfit { index: u8, new_price: Option<u64>, new_size_percent: Option<u64>, new_receive_sol: Option<bool> },
    UpdateStopLoss { index: u8, new_price: Option<u64>, new_size_percent: Option<u64>, new_receive_sol: Option<bool> },
    RemoveTakeProfit { index: u8 },
    RemoveStopLoss { index: u8 },
    ClearAll,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ManageTpSlOrdersParams {
    pub contract_type: u8,
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
    require_eq!(orderbook.contract_type, params.contract_type, TradingError::InvalidOrderType);
    
    // Update borrow fees and position time for perp positions before managing TP/SL orders
    if params.contract_type == 0 {
        let contract = &ctx.accounts.contract;
        let pool = &mut ctx.accounts.pool;
        let position = ctx.accounts.position.as_mut().unwrap();
        let sol_custody = ctx.accounts.sol_custody.as_mut().unwrap();
        let usdc_custody = ctx.accounts.usdc_custody.as_mut().unwrap();
        
        let current_time = contract.get_time()?;
        pool.update_position_borrow_fees(position, current_time, sol_custody, usdc_custody)?;
        
        // Update position timestamp to reflect TP/SL management activity
        position.update_time = current_time;
    }
    
    // Additional validation based on position type
    match params.contract_type {
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
        _ => return Err(TradingError::InvalidOrderType.into()),
    }
    
    // Execute action
    match params.action {
        OrderAction::AddTakeProfit { price, size_percent, receive_sol } => {
            let index = orderbook.add_take_profit_order(price, size_percent, receive_sol)?;
            let (accrued_borrow_fees, last_borrow_fees_update_time, position_side) = if params.contract_type == 0 {
                let position = ctx.accounts.position.as_ref().unwrap();
                (position.accrued_borrow_fees, position.last_borrow_fees_update_time, position.side as u8)
            } else {
                (0, 0, 0) // For options, position_side is not applicable
            };
            emit!(TpSlOrderAdded {
                owner,
                position: orderbook.position,
                contract_type: orderbook.contract_type,
                trigger_order_type: 0, // 0 = TP
                position_side,
                accrued_borrow_fees,
                last_borrow_fees_update_time,
                index: index as u8,
                price,
                size_percent,
                receive_sol,
            });
        },
        OrderAction::AddStopLoss { price, size_percent, receive_sol } => {
            let index = orderbook.add_stop_loss_order(price, size_percent, receive_sol)?;
            let (accrued_borrow_fees, last_borrow_fees_update_time, position_side) = if params.contract_type == 0 {
                let position = ctx.accounts.position.as_ref().unwrap();
                (position.accrued_borrow_fees, position.last_borrow_fees_update_time, position.side as u8)
            } else {
                (0, 0, 0) // For options, position_side is not applicable
            };
            emit!(TpSlOrderAdded {
                owner,
                position: orderbook.position,
                contract_type: orderbook.contract_type,
                trigger_order_type: 1, // 1 = SL
                position_side,
                accrued_borrow_fees,
                last_borrow_fees_update_time,
                index: index as u8,
                price,
                size_percent,
                receive_sol,
            });
        },
        OrderAction::UpdateTakeProfit { index, new_price, new_size_percent, new_receive_sol } => {
            orderbook.update_take_profit_order(index as usize, new_price, new_size_percent, new_receive_sol)?;
            let (accrued_borrow_fees, last_borrow_fees_update_time) = if params.contract_type == 0 {
                let position = ctx.accounts.position.as_ref().unwrap();
                (position.accrued_borrow_fees, position.last_borrow_fees_update_time)
            } else {
                (0, 0)
            };
            emit!(TpSlOrderUpdated {
                owner,
                position: orderbook.position,
                contract_type: orderbook.contract_type,
                trigger_order_type: 0, // 0 = TP
                index,
                accrued_borrow_fees,
                last_borrow_fees_update_time,
                new_price,
                new_size_percent,
                new_receive_sol,
            });
        },
        OrderAction::UpdateStopLoss { index, new_price, new_size_percent, new_receive_sol } => {
            orderbook.update_stop_loss_order(index as usize, new_price, new_size_percent, new_receive_sol)?;
            let (accrued_borrow_fees, last_borrow_fees_update_time) = if params.contract_type == 0 {
                let position = ctx.accounts.position.as_ref().unwrap();
                (position.accrued_borrow_fees, position.last_borrow_fees_update_time)
            } else {
                (0, 0)
            };
            emit!(TpSlOrderUpdated {
                owner,
                position: orderbook.position,
                contract_type: orderbook.contract_type,
                trigger_order_type: 1, // 1 = SL
                index,
                accrued_borrow_fees,
                last_borrow_fees_update_time,
                new_price,
                new_size_percent,
                new_receive_sol,
            });
        },
        OrderAction::RemoveTakeProfit { index } => {
            orderbook.remove_take_profit_order(index as usize)?;
            let (accrued_borrow_fees, last_borrow_fees_update_time) = if params.contract_type == 0 {
                let position = ctx.accounts.position.as_ref().unwrap();
                (position.accrued_borrow_fees, position.last_borrow_fees_update_time)
            } else {
                (0, 0)
            };
            emit!(TpSlOrderRemoved {
                owner,
                position: orderbook.position,
                accrued_borrow_fees,
                last_borrow_fees_update_time,
                contract_type: orderbook.contract_type,
                trigger_order_type: 0, // 0 = TP
                index,
            });
        },
        OrderAction::RemoveStopLoss { index } => {
            orderbook.remove_stop_loss_order(index as usize)?;
            let (accrued_borrow_fees, last_borrow_fees_update_time) = if params.contract_type == 0 {
                let position = ctx.accounts.position.as_ref().unwrap();
                (position.accrued_borrow_fees, position.last_borrow_fees_update_time)
            } else {
                (0, 0)
            };
            emit!(TpSlOrderRemoved {
                owner,
                position: orderbook.position,
                accrued_borrow_fees,
                last_borrow_fees_update_time,
                contract_type: orderbook.contract_type,
                trigger_order_type: 1, // 1 = SL
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
            params.contract_type.to_le_bytes().as_ref(),
        ],
        bump = tp_sl_orderbook.bump
    )]
    pub tp_sl_orderbook: Box<Account<'info, TpSlOrderbook>>,
    
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
    
    // Position account (for perps - only present when contract_type = 0)
    pub position: Option<Box<Account<'info, Position>>>,
    
    // Option account (for options - only present when contract_type = 1)
    pub option_detail: Option<Box<Account<'info, OptionDetail>>>,
    
    // Custody accounts (for perps - only present when contract_type = 0)
    pub sol_custody: Option<Box<Account<'info, Custody>>>,
    pub usdc_custody: Option<Box<Account<'info, Custody>>>,
}
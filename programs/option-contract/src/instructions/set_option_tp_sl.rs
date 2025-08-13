use crate::{
    errors::{OptionError, TradingError},
    events::OptionTpSlSet,
    math::f64_to_scaled_price,
    state::{Contract, OptionDetail, Pool, User},
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SetOptionTpSlParams {
    pub option_index: u64,
    pub pool_name: String,
    pub take_profit_price: Option<f64>,  // Optional take profit price
    pub stop_loss_price: Option<f64>,    // Optional stop loss price
}

pub fn set_option_tp_sl(
    ctx: Context<SetOptionTpSl>,
    params: &SetOptionTpSlParams
) -> Result<()> {
    msg!("Setting TP/SL for option");
    
    let option_detail = &mut ctx.accounts.option_detail;
    let contract = &ctx.accounts.contract;
    
    // Validation
    require_keys_eq!(option_detail.owner, ctx.accounts.owner.key(), TradingError::Unauthorized);
    require!(option_detail.valid, OptionError::InvalidOption);
    require!(!option_detail.executed, OptionError::OptionExecuted);
    
    // Get current time
    let current_time = contract.get_time()?;
    require!(current_time < option_detail.expired_date, OptionError::OptionExpired);
    
    // Convert and validate take profit price
    if let Some(tp_price) = params.take_profit_price {
        require!(tp_price > 0.0, TradingError::InvalidPrice);
        
        // Validate TP price makes sense based on option type
        let strike_price_f64 = option_detail.strike_price as f64 / 1_000_000.0;
        if option_detail.option_type == 0 { // Call option
            require!(tp_price > strike_price_f64, TradingError::InvalidTakeProfitPrice);
        } else { // Put option
            require!(tp_price < strike_price_f64, TradingError::InvalidTakeProfitPrice);
        }
        
        option_detail.take_profit_price = Some(f64_to_scaled_price(tp_price)?);
    } else {
        option_detail.take_profit_price = None;
    }
    
    // Convert and validate stop loss price
    if let Some(sl_price) = params.stop_loss_price {
        require!(sl_price > 0.0, TradingError::InvalidPrice);
        
        // Validate SL price makes sense based on option type
        let strike_price_f64 = option_detail.strike_price as f64 / 1_000_000.0;
        if option_detail.option_type == 0 { // Call option
            require!(sl_price < strike_price_f64, TradingError::InvalidStopLossPrice);
        } else { // Put option
            require!(sl_price > strike_price_f64, TradingError::InvalidStopLossPrice);
        }
        
        option_detail.stop_loss_price = Some(f64_to_scaled_price(sl_price)?);
    } else {
        option_detail.stop_loss_price = None;
    }
    
    // Validate TP and SL don't conflict with each other
    if let (Some(tp), Some(sl)) = (option_detail.take_profit_price, option_detail.stop_loss_price) {
        if option_detail.option_type == 0 { // Call option
            require!(tp > sl, TradingError::InvalidPriceRange);
        } else { // Put option
            require!(tp < sl, TradingError::InvalidPriceRange);
        }
    }
    
    // Update last update time
    option_detail.last_update_time = current_time;
    
    emit!(OptionTpSlSet {
        owner: option_detail.owner,
        index: option_detail.index,
        amount: option_detail.amount,
        quantity: option_detail.quantity,
        period: option_detail.period,
        expired_date: option_detail.expired_date,
        purchase_date: option_detail.purchase_date,
        option_type: option_detail.option_type,
        strike_price: option_detail.strike_price,
        valid: option_detail.valid,
        locked_asset: option_detail.locked_asset,
        pool: option_detail.pool,
        custody: option_detail.custody,
        premium: option_detail.premium,
        premium_asset: option_detail.premium_asset,
        limit_price: option_detail.limit_price,
        executed: option_detail.executed,
        entry_price: option_detail.entry_price,
        last_update_time: option_detail.last_update_time,
        take_profit_price: option_detail.take_profit_price,
        stop_loss_price: option_detail.stop_loss_price,
    });
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: SetOptionTpSlParams)]
pub struct SetOptionTpSl<'info> {
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
        seeds = [b"user_v3", owner.key().as_ref()],
        bump = user.bump
    )]
    pub user: Box<Account<'info, User>>,

    #[account(
        mut,
        seeds = [
            b"option",
            owner.key().as_ref(),
            params.option_index.to_le_bytes().as_ref(),
            pool.key().as_ref(),
            option_detail.custody.as_ref()
        ],
        bump = option_detail.bump
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,
}
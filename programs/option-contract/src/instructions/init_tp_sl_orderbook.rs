use crate::{
    errors::TradingError,
    events::TpSlOrderbookInitialized,
    state::{Pool, Position, OptionDetail, TpSlOrderbook},
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InitTpSlOrderbookParams {
    pub order_type: u8,       // 0 = Perp, 1 = Option
    pub position_index: u64,     // Position or Option index
    pub pool_name: String,
}

pub fn init_tp_sl_orderbook(
    ctx: Context<InitTpSlOrderbook>,
    params: &InitTpSlOrderbookParams
) -> Result<()> {
    msg!("Initializing TP/SL orderbook");
    
    let orderbook = &mut ctx.accounts.tp_sl_orderbook;
    let owner = ctx.accounts.owner.key();
    
    // Initialize based on position type
    match params.order_type {
        0 => {
            // Perp position
            let position = &mut ctx.accounts.position.as_mut().unwrap();
            
            // Validation
            require_keys_eq!(position.owner, owner, TradingError::Unauthorized);
            require!(!position.is_liquidated, TradingError::PositionLiquidated);
            require!(position.tp_sl_orderbook.is_none(), TradingError::OrderbookAlreadyExists);
            
            // Initialize orderbook
            orderbook.initialize(
                owner,
                position.key(),
                params.order_type,
                ctx.bumps.tp_sl_orderbook,
            )?;
            
            // Link position to orderbook
            position.tp_sl_orderbook = Some(orderbook.key());
        },
        1 => {
            // Option position
            let option = &mut ctx.accounts.option_detail.as_mut().unwrap();
            
            // Validation
            require_keys_eq!(option.owner, owner, TradingError::Unauthorized);
            require!(option.valid, TradingError::InvalidOption);
            require!(option.tp_sl_orderbook.is_none(), TradingError::OrderbookAlreadyExists);
            
            // Initialize orderbook
            orderbook.initialize(
                owner,
                option.key(),
                params.order_type,
                ctx.bumps.tp_sl_orderbook,
            )?;
            
            // Link option to orderbook
            option.tp_sl_orderbook = Some(orderbook.key());
        },
        _ => return Err(TradingError::InvalidOrderType.into()),
    }
    
    emit!(TpSlOrderbookInitialized {
        owner,
        position: orderbook.position,
        contract_type: orderbook.contract_type,
        bump: orderbook.bump,
    });
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: InitTpSlOrderbookParams)]
pub struct InitTpSlOrderbook<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    
    #[account(
        init,
        payer = owner,
        space = TpSlOrderbook::LEN,
        seeds = [
            b"tp_sl_orderbook",
            owner.key().as_ref(),
            params.position_index.to_le_bytes().as_ref(),
            params.pool_name.as_bytes(),
            params.order_type.to_le_bytes().as_ref(),
        ],
        bump
    )]
    pub tp_sl_orderbook: Box<Account<'info, TpSlOrderbook>>,
    
    #[account(
        mut,
        seeds = [b"pool", params.pool_name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,
    
    // Position account (for perps - only present when order_type = 0)
    pub position: Option<Box<Account<'info, Position>>>,
    
    // Option account (for options - only present when order_type = 1)  
    pub option_detail: Option<Box<Account<'info, OptionDetail>>>,
    
    pub system_program: Program<'info, System>,
}
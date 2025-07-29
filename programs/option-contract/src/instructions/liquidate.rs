use crate::{
    errors::{PerpetualError, TradingError},
    events::{PositionLiquidated, TpSlOrderbookClosed, PositionAccountClosed},
    math::{self, f64_to_scaled_price},
    state::{Contract, Custody, OraclePrice, Pool, Position, Side, OrderType, TpSlOrderbook},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct LiquidateParams {
    pub position_index: u64,
    pub pool_name: String,
    pub contract_type: u8,
    pub liquidator_reward_account: Pubkey, // Account to receive liquidator reward
}

pub fn liquidate(
    ctx: Context<Liquidate>,
    _params: &LiquidateParams
) -> Result<()> {
    msg!("Liquidating perpetual position");
    
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let position = &mut ctx.accounts.position;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    
    // Validation
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require!(position.order_type == OrderType::Market, PerpetualError::InvalidOrderType);
    
    // Get current prices from oracles
    let current_time = contract.get_time()?;
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let current_sol_price = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();
    let current_price_scaled = f64_to_scaled_price(current_sol_price)?;
    
    msg!("SOL Price: {}", current_sol_price);
    msg!("USDC Price: {}", usdc_price_value);
    msg!("Liquidating position owned by: {}", position.owner);
    msg!("Position entry price: {}", position.price);
    msg!("Position liquidation price: {}", position.liquidation_price);
    msg!("Position side: {:?}", position.side);
    
    // Check if position can be liquidated by price
    let price_liquidatable = position.is_liquidatable(current_price_scaled);
    
    // Check if position can be liquidated by margin ratio
    let margin_liquidatable = position.is_liquidatable_by_margin(current_price_scaled)?;
    
    // Must be liquidatable by either price or margin
    require!(
        price_liquidatable || margin_liquidatable,
        PerpetualError::PositionNotLiquidatable
    );
    
    msg!("Position is eligible for liquidation");
    msg!("Price liquidatable: {}", price_liquidatable);
    msg!("Margin liquidatable: {}", margin_liquidatable);
    
    // Calculate P&L
    let pnl = position.calculate_pnl(current_price_scaled)?;
    
    // Update accrued borrow fees before liquidation
    let interest_payment: u64 = pool.update_position_borrow_fees(
        position, 
        current_time, 
        sol_custody, 
        usdc_custody
    )?;
    
    // Calculate liquidator reward (0.5% of position size)
    let liquidator_reward_usd = 0; // 0.5%
    
    // Calculate net settlement after all deductions
    let mut net_settlement = position.collateral_usd as i64 + pnl as i64 - interest_payment as i64 - liquidator_reward_usd as i64 - position.trade_fees as i64;
    
    // Ensure settlement is not negative
    if net_settlement < 0 {
        net_settlement = 0;
    }
    
    let settlement_usd = net_settlement as u64;
    
    msg!("P&L: {}", pnl);
    msg!("Interest payment: {}", interest_payment);
    msg!("Liquidator reward USD: {}", liquidator_reward_usd);
    msg!("Net settlement USD: {}", settlement_usd);
    
    // Calculate settlement amounts in tokens
    let (collateral_price, collateral_decimals) = if position.collateral_custody == sol_custody.key() {
        (current_sol_price, sol_custody.decimals)
    } else {
        (usdc_price_value, usdc_custody.decimals)
    };
    
    // Settlement to position owner
    let settlement_tokens = if settlement_usd > 0 {
        let amount = math::checked_as_u64(settlement_usd as f64 / collateral_price)?;
        math::checked_as_u64(amount as f64 * math::checked_powi(10.0, collateral_decimals as i32)? / 1_000_000.0)?
    } else {
        0
    };
    
    // Liquidator reward tokens
    let liquidator_reward_tokens = if liquidator_reward_usd > 0 {
        let amount = math::checked_as_u64(liquidator_reward_usd as f64 / collateral_price)?;
        math::checked_as_u64(amount as f64 * math::checked_powi(10.0, collateral_decimals as i32)?)?
    } else {
        0
    };
    
    // Transfer settlement to position owner if any
    if settlement_tokens > 0 {
        ctx.accounts.contract.transfer_tokens(
            if position.collateral_custody == sol_custody.key() {
                ctx.accounts.sol_custody_token_account.to_account_info()
            } else {
                ctx.accounts.usdc_custody_token_account.to_account_info()
            },
            ctx.accounts.owner_settlement_account.to_account_info(),
            ctx.accounts.transfer_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            settlement_tokens,
        )?;
    }
    
    // Transfer liquidator reward
    if liquidator_reward_tokens > 0 {
        ctx.accounts.contract.transfer_tokens(
            if position.collateral_custody == sol_custody.key() {
                ctx.accounts.sol_custody_token_account.to_account_info()
            } else {
                ctx.accounts.usdc_custody_token_account.to_account_info()
            },
            ctx.accounts.liquidator_reward_account.to_account_info(),
            ctx.accounts.transfer_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            liquidator_reward_tokens,
        )?;
    }
    
    // Update custody stats - release locked tokens
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
    
    // Update custody ownership - remove collateral
    if position.collateral_custody == sol_custody.key() {
        sol_custody.token_owned = math::checked_sub(
            sol_custody.token_owned,
            position.collateral_amount
        )?;
    } else {
        usdc_custody.token_owned = math::checked_sub(
            usdc_custody.token_owned,
            position.collateral_amount
        )?;
    }
    
    // Update pool open interest
    if position.side == Side::Long {
        pool.long_open_interest_usd = math::checked_sub(pool.long_open_interest_usd, position.size_usd as u128)?;
    } else {
        pool.short_open_interest_usd = math::checked_sub(pool.short_open_interest_usd, position.size_usd as u128)?;
    }
    
    // Store values before modifying position for event emission and account closure
    let position_owner = position.owner;
    let position_key = position.key();
    let position_pool = position.pool;
    
    // Mark position as liquidated (fully closed)
    position.is_liquidated = true;
    position.size_usd = 0;
    position.collateral_amount = 0;
    position.collateral_usd = 0;
    position.locked_amount = 0;
    position.update_time = current_time;
    
    // Clear all remaining TP/SL orders in orderbook if it exists
    if let Some(orderbook_info) = ctx.accounts.tp_sl_orderbook.as_ref() {
        // Validate the orderbook account if provided
        let position_index_bytes = _params.position_index.to_le_bytes();
        let contract_type_bytes = _params.contract_type.to_le_bytes();
        let expected_seeds = [
            b"tp_sl_orderbook",
            position_owner.as_ref(),
            position_index_bytes.as_ref(),
            _params.pool_name.as_bytes(),
            contract_type_bytes.as_ref(),
        ];
        let (expected_key, _) = Pubkey::find_program_address(&expected_seeds, ctx.program_id);
        require_keys_eq!(orderbook_info.key(), expected_key, TradingError::Unauthorized);
        
        // Check if account is initialized (has data and correct discriminator)
        let orderbook_data = orderbook_info.try_borrow_data()?;
        if orderbook_data.len() >= 8 {
            // Try to deserialize - if it fails, the account is not properly initialized
            if let Ok(_orderbook) = TpSlOrderbook::try_deserialize(&mut orderbook_data.as_ref()) {
                drop(orderbook_data); // Release the borrow
                
                // Account is valid, clear orders
                let mut orderbook_data = orderbook_info.try_borrow_mut_data()?;
                let mut orderbook = TpSlOrderbook::try_deserialize(&mut orderbook_data.as_ref())?;
                orderbook.clear_all_orders()?;
                
                // Serialize back
                orderbook.try_serialize(&mut orderbook_data.as_mut())?;
            }
        }
    }
    
    msg!("Position fully liquidated - will automatically close TP/SL orderbook and position accounts");

    let interest_u64 = interest_payment.try_into().unwrap();
    
    position.borrow_fees_paid = math::checked_add(position.borrow_fees_paid, interest_u64)?;
    position.accrued_borrow_fees = math::checked_sub(position.accrued_borrow_fees, interest_u64)?;
    
    emit!(PositionLiquidated {
        pub_key: position_key,
        index: position.index,
        owner: position_owner,
        pool: position_pool,
        custody: position.custody,
        collateral_custody: position.collateral_custody,
        order_type: position.order_type as u8,
        side: position.side as u8,
        is_liquidated: position.is_liquidated,
        price: position.price,
        size_usd: position.size_usd,
        collateral_usd: position.collateral_usd,
        open_time: position.open_time,
        update_time: position.update_time,
        liquidation_price: position.liquidation_price,
        cumulative_interest_snapshot: position.cumulative_interest_snapshot,
        trade_fees: 0,
        trade_fees_paid: position.trade_fees,
        borrow_fees_paid: interest_payment.try_into().unwrap(),
        accrued_borrow_fees: position.accrued_borrow_fees,
        last_borrow_fees_update_time: position.last_borrow_fees_update_time,
        locked_amount: position.locked_amount,
        collateral_amount: position.collateral_amount,
        trigger_price: position.trigger_price,
        trigger_above_threshold: position.trigger_above_threshold,
        bump: position.bump,
        settlement_tokens,
        pnl: pnl,
        liquidator_reward_tokens,
        liquidator: ctx.accounts.liquidator.key(),
    });
    
    // Automatically close accounts - TP/SL orderbook first if it exists and is initialized
    if let Some(orderbook_info) = ctx.accounts.tp_sl_orderbook.as_ref() {
        // Only close if the account has data (is initialized)
        let orderbook_data = orderbook_info.try_borrow_data()?;
        if orderbook_data.len() >= 8 {
            let orderbook_rent = orderbook_info.lamports();
            drop(orderbook_data); // Release the borrow
            
            **orderbook_info.try_borrow_mut_lamports()? = 0;
            **ctx.accounts.liquidator.to_account_info().try_borrow_mut_lamports()? = ctx.accounts.liquidator
                .to_account_info()
                .lamports()
                .checked_add(orderbook_rent)
                .ok_or(ProgramError::ArithmeticOverflow)?;
                
            // Clear orderbook data
            {
                let mut orderbook_data = orderbook_info.try_borrow_mut_data()?;
                orderbook_data.fill(0);
            }
            
            emit!(TpSlOrderbookClosed {
                owner: position_owner,
                position: position_key,
                contract_type: _params.contract_type,
                rent_refunded: orderbook_rent,
            });
        }
    }
    
    // Close position account
    let position_rent = ctx.accounts.position.to_account_info().lamports();
    **ctx.accounts.position.to_account_info().try_borrow_mut_lamports()? = 0;
    **ctx.accounts.liquidator.to_account_info().try_borrow_mut_lamports()? = ctx.accounts.liquidator
        .to_account_info()
        .lamports()
        .checked_add(position_rent)
        .ok_or(ProgramError::ArithmeticOverflow)?;
        
    // Clear position data
    {
        let position_info = ctx.accounts.position.to_account_info();
        let mut position_data = position_info.try_borrow_mut_data()?;
        position_data.fill(0);
    }
    
    emit!(PositionAccountClosed {
        owner: position_owner,
        position_key,
        position_index: _params.position_index,
        pool: position_pool,
        rent_refunded: position_rent,
    });
    
    msg!("Position and TP/SL orderbook accounts automatically closed - all rent returned to liquidator");
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: LiquidateParams)]
pub struct Liquidate<'info> {
    #[account(mut)]
    pub liquidator: Signer<'info>,

    /// CHECK: Position owner for settlement
    #[account(mut)]
    pub owner_settlement_account: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub liquidator_reward_account: Box<Account<'info, TokenAccount>>,

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

    /// CHECK: Oracle account validation is handled by constraint
    #[account(
        constraint = sol_oracle_account.key() == sol_custody.oracle
    )]
    pub sol_oracle_account: AccountInfo<'info>,

    /// CHECK: Oracle account validation is handled by constraint
    #[account(
        constraint = usdc_oracle_account.key() == usdc_custody.oracle
    )]
    pub usdc_oracle_account: AccountInfo<'info>,

    #[account(mut)]
    pub sol_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub usdc_mint: Box<Account<'info, Mint>>,

    /// CHECK: Optional TP/SL orderbook account - may not exist if user never set TP/SL
    pub tp_sl_orderbook: Option<AccountInfo<'info>>,

    pub token_program: Program<'info, Token>,
}
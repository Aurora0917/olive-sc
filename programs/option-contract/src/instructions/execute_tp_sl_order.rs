use crate::{
    errors::{PerpetualError, TradingError},
    events::{PositionAccountClosed, TpSlOrderExecuted, TpSlOrderbookClosed},
    math::{self, f64_to_scaled_price},
    state::{Contract, Custody, OraclePrice, Pool, Position, Side, TpSlOrderbook},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ExecuteTpSlOrderParams {
    pub position_index: u64,
    pub pool_name: String,
    pub contract_type: u8,
    pub trigger_order_type: u8, // 0 = TP, 1 = SL
    pub order_index: u8,
}

pub fn execute_tp_sl_order(
    ctx: Context<ExecuteTpSlOrder>,
    params: &ExecuteTpSlOrderParams,
) -> Result<()> {
    msg!("Executing TP/SL order - dedicated instruction for keeper");

    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let position = &mut ctx.accounts.position;
    let orderbook = &mut ctx.accounts.tp_sl_orderbook;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;

    // Validation
    require_keys_eq!(position.owner, orderbook.owner, TradingError::Unauthorized);
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require_eq!(
        orderbook.contract_type,
        params.contract_type,
        TradingError::InvalidOrderType
    );
    require_eq!(
        orderbook.position,
        position.key(),
        TradingError::InvalidPosition
    );

    // Get current time and prices
    let current_time = contract.get_time()?;
    let sol_price =
        OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price =
        OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;

    let current_sol_price = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();
    let current_price_scaled = f64_to_scaled_price(current_sol_price)?;

    msg!("Current SOL price from oracle: {}", current_sol_price);

    // Update borrow fees before executing TP/SL order
    let interest_payment =
        pool.update_position_borrow_fees(position, current_time, sol_custody, usdc_custody)?;

    // Get the TP/SL order to execute
    let (order_price, size_percent, receive_sol) = if params.trigger_order_type == 0 {
        // Take Profit
        require!(
            params.order_index < orderbook.take_profit_orders.len() as u8,
            TradingError::InvalidAmount
        );
        let order = &orderbook.take_profit_orders[params.order_index as usize];
        require!(order.is_active, TradingError::InvalidAmount);
        (order.price, order.size_percent, order.receive_sol)
    } else {
        // Stop Loss
        require!(
            params.order_index < orderbook.stop_loss_orders.len() as u8,
            TradingError::InvalidAmount
        );
        let order = &orderbook.stop_loss_orders[params.order_index as usize];
        require!(order.is_active, TradingError::InvalidAmount);
        (order.price, order.size_percent, order.receive_sol)
    };

    // Validate execution conditions using oracle price
    if params.trigger_order_type == 0 {
        // Take Profit
        match position.side {
            Side::Long => require!(
                current_price_scaled >= order_price,
                PerpetualError::TpSlNotTriggered
            ),
            Side::Short => require!(
                current_price_scaled <= order_price,
                PerpetualError::TpSlNotTriggered
            ),
        }
    } else {
        // Stop Loss
        match position.side {
            Side::Long => require!(
                current_price_scaled <= order_price,
                PerpetualError::TpSlNotTriggered
            ),
            Side::Short => require!(
                current_price_scaled >= order_price,
                PerpetualError::TpSlNotTriggered
            ),
        }
    }

    // Calculate close percentage from size_percent (basis points)
    let close_percentage = size_percent as f64 / 100.0; // Convert basis points to percentage
    let is_full_close = close_percentage >= 100.0;

    // Calculate P&L using oracle price
    let pnl = position.calculate_pnl(current_price_scaled)?;

    // Calculate amounts to close (proportional to percentage)
    let close_ratio = if is_full_close {
        1.0
    } else {
        close_percentage / 100.0
    };
    let size_usd_to_close = if is_full_close {
        position.size_usd
    } else {
        math::checked_as_u64(position.size_usd as f64 * close_ratio)?
    };

    let collateral_amount_to_close = if is_full_close {
        position.collateral_amount
    } else {
        math::checked_as_u64(position.collateral_amount as f64 * close_ratio)?
    };

    let collateral_usd_to_close = if is_full_close {
        position.collateral_usd
    } else {
        math::checked_as_u64(position.collateral_usd as f64 * close_ratio)?
    };

    // Calculate P&L and interest for the portion being closed
    let pnl_for_closed_portion = if is_full_close {
        pnl
    } else {
        (pnl as f64 * close_ratio) as i64
    };

    let interest_for_closed_portion = if is_full_close {
        interest_payment
    } else {
        math::checked_as_u64(interest_payment as f64 * close_ratio)?
    };

    let trade_fees_for_closed_portion = if is_full_close {
        position.trade_fees
    } else {
        math::checked_as_u64(position.trade_fees as f64 * close_ratio)?
    };

    let mut net_settlement = collateral_usd_to_close as i64 + pnl_for_closed_portion
        - interest_for_closed_portion as i64
        - trade_fees_for_closed_portion as i64;

    // Ensure settlement is not negative
    if net_settlement < 0 {
        net_settlement = 0;
    }

    let settlement_usd = net_settlement as u64;

    // Calculate settlement amount in requested asset
    let (settlement_amount, settlement_decimals) = if receive_sol {
        let amount = math::checked_as_u64(settlement_usd as f64 / current_sol_price)?;
        (amount, sol_custody.decimals)
    } else {
        let amount = math::checked_as_u64(settlement_usd as f64 / usdc_price_value)?;
        (amount, usdc_custody.decimals)
    };

    // Adjust for token decimals
    let settlement_tokens = math::checked_as_u64(
        settlement_amount as f64 * math::checked_powi(10.0, settlement_decimals as i32)?
            / 1_000_000.0,
    )?;

    let (native_exit_amount, native_exit_decimals) = if position.side == Side::Long {
        let amount = math::checked_as_u64(settlement_usd as f64 / current_sol_price)?;
        (amount, sol_custody.decimals)
    } else {
        let amount = math::checked_as_u64(settlement_usd as f64 / usdc_price_value)?;
        (amount, usdc_custody.decimals)
    };

    let native_exit_tokens = math::checked_as_u64(
        native_exit_amount as f64 * math::checked_powi(10.0, native_exit_decimals as i32)?
            / 1_000_000.0,
    )?;

    // Transfer settlement to user
    if settlement_tokens > 0 {
        ctx.accounts.contract.transfer_tokens(
            if receive_sol {
                ctx.accounts.sol_custody_token_account.to_account_info()
            } else {
                ctx.accounts.usdc_custody_token_account.to_account_info()
            },
            ctx.accounts.receiving_account.to_account_info(),
            ctx.accounts.transfer_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            settlement_tokens,
        )?;
    }

    // Update custody stats
    let locked_amount_to_release = if is_full_close {
        position.locked_amount
    } else {
        math::checked_as_u64(position.locked_amount as f64 * close_ratio)?
    };

    if position.side == Side::Long {
        sol_custody.token_locked =
            math::checked_sub(sol_custody.token_locked, locked_amount_to_release)?;
    } else {
        usdc_custody.token_locked =
            math::checked_sub(usdc_custody.token_locked, locked_amount_to_release)?;
    }

    // Update custody ownership
    if position.collateral_custody == sol_custody.key() {
        sol_custody.token_owned =
            math::checked_sub(sol_custody.token_owned, collateral_amount_to_close)?;
    } else {
        usdc_custody.token_owned =
            math::checked_sub(usdc_custody.token_owned, collateral_amount_to_close)?;
    }

    // Update pool open interest
    if position.side == Side::Long {
        pool.long_open_interest_usd =
            math::checked_sub(pool.long_open_interest_usd, size_usd_to_close as u128)?;
    } else {
        pool.short_open_interest_usd =
            math::checked_sub(pool.short_open_interest_usd, size_usd_to_close as u128)?;
    }

    // Store position values before modification for event emission
    let position_owner = position.owner;
    let position_key = position.key();
    let position_pool = position.pool;
    let position_custody = position.custody;
    let position_collateral_custody = position.collateral_custody;
    let position_order_type = position.order_type as u8;
    let position_side = position.side as u8;
    let position_entry_price = position.price;
    let position_size_usd = position.size_usd;
    let position_collateral_usd = position.collateral_usd;
    let position_collateral_amount = position.collateral_amount;
    let position_locked_amount = position.locked_amount;
    let position_open_time = position.open_time;
    let position_liquidation_price = position.liquidation_price;
    let position_cumulative_interest_snapshot = position.cumulative_interest_snapshot;
    let position_trade_fees = position.trade_fees;
    let position_accrued_borrow_fees = position.accrued_borrow_fees;
    let position_last_borrow_fees_update_time = position.last_borrow_fees_update_time;

    // Mark the order as executed in the orderbook FIRST before any other changes
    if params.trigger_order_type == 0 {
        orderbook.mark_tp_executed(params.order_index as usize, current_time)?;
    } else {
        orderbook.mark_sl_executed(params.order_index as usize, current_time)?;
    }

    // Update or close position
    if is_full_close {
        msg!("Position fully closed - triggering automatic account closures");

        position.is_liquidated = true; // Mark as closed
        position.size_usd = 0;
        position.collateral_amount = 0;
        position.collateral_usd = 0;
        position.locked_amount = 0;
        position.trade_fees = 0;

        // Clear all remaining TP/SL orders in orderbook (executed order already marked above)
        orderbook.clear_all_orders()?;

        // Position fully closed - will automatically close both accounts and return rent
        msg!("Position fully closed - will automatically close TP/SL orderbook and position accounts");
    } else {
        // Update position for partial close
        position.size_usd = math::checked_sub(position.size_usd, size_usd_to_close)?;
        position.collateral_amount =
            math::checked_sub(position.collateral_amount, collateral_amount_to_close)?;
        position.collateral_usd =
            math::checked_sub(position.collateral_usd, collateral_usd_to_close)?;
        position.locked_amount =
            math::checked_sub(position.locked_amount, locked_amount_to_release)?;
        position.trade_fees =
            math::checked_sub(position.trade_fees, trade_fees_for_closed_portion)?;
    }

    // Update fee tracking
    position.borrow_fees_paid =
        math::checked_add(position.borrow_fees_paid, interest_for_closed_portion)?;
    position.accrued_borrow_fees =
        math::checked_sub(position.accrued_borrow_fees, interest_for_closed_portion)?;
    position.update_time = current_time;

    emit!(TpSlOrderExecuted {
        // Position identification
        position_index: params.position_index,
        position_key,
        owner: position_owner,
        pool: position_pool,

        // Position details
        custody: position_custody,
        collateral_custody: position_collateral_custody,
        order_type: position_order_type,
        side: position_side,
        is_liquidated: is_full_close,
        entry_price: position_entry_price,
        size_usd: position_size_usd,
        collateral_usd: position_collateral_usd,
        collateral_amount: position_collateral_amount,
        native_exit_amount: native_exit_tokens,
        locked_amount: position_locked_amount,

        // TP/SL specific
        contract_type: params.contract_type,
        trigger_order_type: params.trigger_order_type,
        order_index: params.order_index,
        order_price,
        executed_price: current_price_scaled,
        executed_size_percent: size_percent,
        receive_sol,

        // Fees and PnL
        trade_fees: position_trade_fees,
        trade_fees_paid: trade_fees_for_closed_portion,
        borrow_fees_paid: interest_for_closed_portion,
        accrued_borrow_fees: position_accrued_borrow_fees,
        realized_pnl: pnl_for_closed_portion,
        settlement_tokens,

        // Timestamps
        open_time: position_open_time,
        update_time: current_time,
        executed_at: current_time,
        last_borrow_fees_update_time: position_last_borrow_fees_update_time,

        // Additional info
        liquidation_price: position_liquidation_price,
        cumulative_interest_snapshot: position_cumulative_interest_snapshot,
        is_full_close,
    });

    // Automatically close accounts if position was fully closed
    if is_full_close {
        // Close TP/SL orderbook first
        let orderbook_rent = ctx.accounts.tp_sl_orderbook.to_account_info().lamports();
        **ctx
            .accounts
            .tp_sl_orderbook
            .to_account_info()
            .try_borrow_mut_lamports()? = 0;
        **ctx
            .accounts
            .executor
            .to_account_info()
            .try_borrow_mut_lamports()? = ctx
            .accounts
            .executor
            .to_account_info()
            .lamports()
            .checked_add(orderbook_rent)
            .ok_or(ProgramError::ArithmeticOverflow)?;

        // Clear orderbook data
        {
            let orderbook_info = ctx.accounts.tp_sl_orderbook.to_account_info();
            let mut orderbook_data = orderbook_info.try_borrow_mut_data()?;
            orderbook_data.fill(0);
        }

        emit!(TpSlOrderbookClosed {
            owner: position_owner,
            position: position_key,
            contract_type: params.contract_type,
            rent_refunded: orderbook_rent,
        });

        // Close position account
        let position_rent = ctx.accounts.position.to_account_info().lamports();
        **ctx
            .accounts
            .position
            .to_account_info()
            .try_borrow_mut_lamports()? = 0;
        **ctx
            .accounts
            .executor
            .to_account_info()
            .try_borrow_mut_lamports()? = ctx
            .accounts
            .executor
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
            position_index: params.position_index,
            pool: position_pool,
            rent_refunded: position_rent,
        });

        msg!("TP/SL orderbook and position accounts automatically closed - all rent returned to user");
    }

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: ExecuteTpSlOrderParams)]
pub struct ExecuteTpSlOrder<'info> {
    #[account(mut)]
    pub executor: Signer<'info>, // Keeper can execute

    #[account(
        mut,
        constraint = receiving_account.owner == tp_sl_orderbook.owner
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
        seeds = [
            b"tp_sl_orderbook",
            tp_sl_orderbook.owner.as_ref(),
            params.position_index.to_le_bytes().as_ref(),
            params.pool_name.as_bytes(),
            params.contract_type.to_le_bytes().as_ref(),
        ],
        bump = tp_sl_orderbook.bump
    )]
    pub tp_sl_orderbook: Box<Account<'info, TpSlOrderbook>>,

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

    pub token_program: Program<'info, Token>,
}

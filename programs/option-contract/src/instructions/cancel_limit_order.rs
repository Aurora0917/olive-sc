use crate::{
    errors::{PerpetualError, TradingError},
    events::{LimitOrderCanceled, PositionAccountClosed, TpSlOrderbookClosed},
    math,
    state::{Contract, Custody, OraclePrice, OrderType, Pool, Position, TpSlOrderbook},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CancelLimitOrderParams {
    pub position_index: u64,
    pub pool_name: String,
    pub contract_type: u8,
    pub close_percentage: u8, // 1-100: 100 = full close, <100 = partial close
    pub receive_sol: bool,    // true = receive SOL, false = receive USDC
}

pub fn cancel_limit_order(
    ctx: Context<CancelLimitOrder>,
    params: &CancelLimitOrderParams,
) -> Result<()> {
    msg!("Canceling {}% of limit order", params.close_percentage);

    let contract = &ctx.accounts.contract;
    let position = &mut ctx.accounts.position;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;

    // Validation
    require_keys_eq!(
        position.owner,
        ctx.accounts.owner.key(),
        TradingError::Unauthorized
    );
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require!(
        position.order_type == OrderType::Limit,
        PerpetualError::NotLimitOrder
    );
    require!(position.size_usd > 0, PerpetualError::InvalidPositionSize);
    require!(
        params.close_percentage > 0 && params.close_percentage <= 100,
        TradingError::InvalidAmount
    );

    let current_time = contract.get_time()?;
    let is_full_close = params.close_percentage == 100;

    msg!("Canceling limit order for position:");
    msg!("Position size USD: {}", position.size_usd);
    msg!("Collateral USD: {}", position.collateral_usd);
    msg!("Collateral amount: {}", position.collateral_amount);
    msg!("Position side: {:?}", position.side);
    msg!(
        "User chose to receive: {}",
        if params.receive_sol { "SOL" } else { "USDC" }
    );
    msg!("Close percentage: {}%", params.close_percentage);

    // Calculate amounts to cancel (proportional to percentage)
    let close_ratio = params.close_percentage as f64 / 100.0;

    let size_usd_to_cancel = if is_full_close {
        position.size_usd
    } else {
        math::checked_as_u64(position.size_usd as f64 * close_ratio)?
    };

    let collateral_amount_to_refund = if is_full_close {
        position.collateral_amount
    } else {
        math::checked_as_u64(position.collateral_amount as f64 * close_ratio)?
    };

    let collateral_usd_to_refund = if is_full_close {
        position.collateral_usd
    } else {
        math::checked_as_u64(position.collateral_usd as f64 * close_ratio)?
    };

    let locked_amount_to_release = if is_full_close {
        position.locked_amount
    } else {
        math::checked_as_u64(position.locked_amount as f64 * close_ratio)?
    };

    msg!("Size USD to cancel: {}", size_usd_to_cancel);
    msg!(
        "Collateral amount to refund: {}",
        collateral_amount_to_refund
    );
    msg!("Collateral USD to refund: {}", collateral_usd_to_refund);
    msg!("Locked amount to release: {}", locked_amount_to_release);

    // Store custody keys first to avoid borrowing issues
    let sol_custody_key = sol_custody.key();
    let _usdc_custody_key = usdc_custody.key();

    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let current_sol_price = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();

    let (settlement_amount, settlement_decimals) = if params.receive_sol {
        let amount = math::checked_as_u64(collateral_usd_to_refund as f64 / current_sol_price)?;
        (amount, sol_custody.decimals)
    } else {
        let amount = math::checked_as_u64(collateral_usd_to_refund as f64 / usdc_price_value)?;
        (amount, usdc_custody.decimals)
    };

    // Adjust for token decimals
    let settlement_tokens = math::checked_as_u64(
        settlement_amount as f64 * math::checked_powi(10.0, settlement_decimals as i32)?
            / 1_000_000.0,
    )?;

    // Transfer collateral back to user
    if collateral_amount_to_refund > 0 {
        // Determine which token account to use for transfer
        let original_token_account = if params.receive_sol {
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
            settlement_tokens,
        )?;

        // Update custody stats - remove collateral from pool
        if position.collateral_custody == sol_custody_key {
            sol_custody.token_owned =
                math::checked_sub(sol_custody.token_owned, collateral_amount_to_refund)?;
        } else {
            usdc_custody.token_owned =
                math::checked_sub(usdc_custody.token_owned, collateral_amount_to_refund)?;
        }
        
        // Note: For limit orders, tokens were never locked at custody level when opened,
        // so we don't need to release any locked tokens when canceling.
        // Only executed limit orders have locked tokens, and they use close_perp_position to release.
    }

    // Store position values before modification for account closure
    let position_owner = position.owner;
    let position_key = position.key();
    let position_pool = position.pool;
    
    // Update or close position
    if is_full_close {
        msg!("Limit order fully canceled - will automatically close TP/SL orderbook and position accounts");
        
        // Clear all remaining TP/SL orders in orderbook if it exists
        if let Some(orderbook_info) = ctx.accounts.tp_sl_orderbook.as_ref() {
            // Validate the orderbook account if provided
            let position_index_bytes = params.position_index.to_le_bytes();
            let contract_type_bytes = params.contract_type.to_le_bytes();
            let expected_seeds = [
                b"tp_sl_orderbook",
                position_owner.as_ref(),
                position_index_bytes.as_ref(),
                params.pool_name.as_bytes(),
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
        
        // Mark position as liquidated (canceled)
        position.is_liquidated = true;
        position.size_usd = 0;
        position.collateral_amount = 0;
        position.collateral_usd = 0;
        position.locked_amount = 0;
        position.trigger_price = None;
        position.order_type = OrderType::Market; // Reset to market for cleanup
    } else {
        // Update position for partial cancellation
        position.size_usd = math::checked_sub(position.size_usd, size_usd_to_cancel)?;
        position.collateral_amount =
            math::checked_sub(position.collateral_amount, collateral_amount_to_refund)?;
        position.collateral_usd =
            math::checked_sub(position.collateral_usd, collateral_usd_to_refund)?;
        position.locked_amount =
            math::checked_sub(position.locked_amount, locked_amount_to_release)?;

        // Keep the limit order type and trigger price for remaining position
    }

    position.update_time = current_time;

    // No fees for canceling limit orders since they were never active positions

    emit!(LimitOrderCanceled {
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
        trade_fees: position.trade_fees,
        borrow_fees_paid: position.borrow_fees_paid,
        locked_amount: position.locked_amount,
        collateral_amount: position.collateral_amount,
        trigger_price: position.trigger_price,
        trigger_above_threshold: position.trigger_above_threshold,
        bump: position.bump,
        close_percentage: params.close_percentage as u64,
        refunded_collateral: collateral_amount_to_refund,
        refunded_collateral_usd: collateral_usd_to_refund,
    });
    
    // Automatically close accounts if fully canceled
    if is_full_close {
        // Close TP/SL orderbook first if it exists and is initialized
        if let Some(orderbook_info) = ctx.accounts.tp_sl_orderbook.as_ref() {
            // Only close if the account has data (is initialized)
            let orderbook_data = orderbook_info.try_borrow_data()?;
            if orderbook_data.len() >= 8 {
                let orderbook_rent = orderbook_info.lamports();
                drop(orderbook_data); // Release the borrow
                
                **orderbook_info.try_borrow_mut_lamports()? = 0;
                **ctx.accounts.owner.to_account_info().try_borrow_mut_lamports()? = ctx.accounts.owner
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
                    contract_type: params.contract_type,
                    rent_refunded: orderbook_rent,
                });
            }
        }
        
        // Close position account
        let position_rent = ctx.accounts.position.to_account_info().lamports();
        **ctx.accounts.position.to_account_info().try_borrow_mut_lamports()? = 0;
        **ctx.accounts.owner.to_account_info().try_borrow_mut_lamports()? = ctx.accounts.owner
            .to_account_info()
            .lamports()
            .checked_add(position_rent)
            .ok_or(ProgramError::ArithmeticOverflow)?;
            
        // Clear position account data
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

    /// CHECK: Optional TP/SL orderbook account - may not exist if user never set TP/SL
    pub tp_sl_orderbook: Option<AccountInfo<'info>>,

    pub token_program: Program<'info, Token>,
}

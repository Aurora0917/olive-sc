use crate::{
    errors::{PerpetualError, TradingError},
    events::{PerpPositionClosed, PositionAccountClosed, TpSlOrderbookClosed},
    math::{self, f64_to_scaled_price},
    state::{Contract, Custody, OraclePrice, Pool, Position, Side, OrderType, TpSlOrderbook},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ClosePerpPositionParams {
    pub position_index: u64,
    pub pool_name: String,
    pub contract_type: u8,
    pub close_percentage: u64,
    pub receive_sol: bool,          // true = receive SOL, false = receive USDC
}

pub fn close_perp_position(
    ctx: Context<ClosePerpPosition>,
    params: &ClosePerpPositionParams
) -> Result<()> {
    msg!("Closing {}% of perpetual position", params.close_percentage);
    // Note: This instruction is used by both users and keepers for TP/SL execution
    
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let position = &mut ctx.accounts.position;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    
    // Validation
    require_keys_eq!(position.owner, ctx.accounts.owner.key(), TradingError::Unauthorized);
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require!(position.order_type == OrderType::Market, PerpetualError::InvalidOrderType);
    require!(
        params.close_percentage > 0 && params.close_percentage <= 100_000_000,
        TradingError::InvalidAmount
    );

    let is_full_close = params.close_percentage == 100_000_000;
    
    // Get current prices from oracles
    let current_time = contract.get_time()?;
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let current_sol_price = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();
    
    msg!("SOL Price: {}", current_sol_price);
    msg!("USDC Price: {}", usdc_price_value);
    msg!("Closing at SOL price: ${}", current_sol_price);
    msg!("User chose to receive: {}", if params.receive_sol { "SOL" } else { "USDC" });
    msg!("Position side {:?}",  position.side);
    
    // Slippage protection
    let current_price_scaled = f64_to_scaled_price(current_sol_price)?;
    
    // Calculate P&L
    let pnl = position.calculate_pnl(current_price_scaled)?;
    
    // Update accrued borrow fees before closing position
    let interest_payment: u64 = pool.update_position_borrow_fees(
        position, 
        current_time, 
        sol_custody, 
        usdc_custody
    )?;
    
    // Calculate amounts to close (proportional to percentage) - using integer math
    let size_usd_to_close = if is_full_close {
        position.size_usd
    } else {
        math::checked_as_u64(math::checked_div(
            math::checked_mul(position.size_usd as u128, params.close_percentage as u128)?,
            100_000_000u128
        )?)?
    };
    
    let collateral_amount_to_close = if is_full_close {
        position.collateral_amount
    } else {
        math::checked_as_u64(math::checked_div(
            math::checked_mul(position.collateral_amount as u128, params.close_percentage as u128)?,
            100_000_000u128
        )?)?
    };
    
    let collateral_usd_to_close = if is_full_close {
        position.collateral_usd
    } else {
        math::checked_as_u64(math::checked_div(
            math::checked_mul(position.collateral_usd as u128, params.close_percentage as u128)?,
            100_000_000u128
        )?)?
    };
    
    // Calculate P&L, funding, and interest for the portion being closed
    let pnl_for_closed_portion = if is_full_close {
        pnl
    } else {
        // Use integer math for PnL calculation
        if pnl >= 0 {
            math::checked_as_i64(math::checked_div(
                math::checked_mul(pnl as u128, params.close_percentage as u128)?,
                100_000_000u128
            )?)?
        } else {
            -math::checked_as_i64(math::checked_div(
                math::checked_mul((-pnl) as u128, params.close_percentage as u128)?,
                100_000_000u128
            )?)?
        }
    };
    
    let interest_for_closed_portion = if is_full_close {
        interest_payment
    } else {
        math::checked_as_u64(math::checked_div(
            math::checked_mul(interest_payment as u128, params.close_percentage as u128)?,
            100_000_000u128
        )?)?
    };

    let trade_fees_for_closed_portion = if is_full_close {
        position.trade_fees
    } else {
        math::checked_as_u64(math::checked_div(
            math::checked_mul(position.trade_fees as u128, params.close_percentage as u128)?,
            100_000_000u128
        )?)?
    }; 
    
    msg!("Size USD to close: {}", size_usd_to_close);
    msg!("Collateral amount to close: {}", collateral_amount_to_close);
    msg!("P&L for closed portion: {}", pnl_for_closed_portion);
    msg!("Interest for closed portion: {}", interest_for_closed_portion);
    
    let mut net_settlement = collateral_usd_to_close as i64 + pnl_for_closed_portion - interest_for_closed_portion as i64 - trade_fees_for_closed_portion as i64;
    
    // Ensure settlement is not negative
    if net_settlement < 0 {
        net_settlement = 0;
    }
    
    let settlement_usd = net_settlement as u64;
    
    // Calculate settlement amount in requested asset using integer math
    let settlement_tokens = if params.receive_sol {
        // Scale SOL price to 6 decimals for consistent math
        let sol_price_scaled = sol_price.scale_to_exponent(-6)?;
        
        // USD amount / SOL price = SOL amount (both with 6 decimals)
        let sol_amount_6_decimals = math::checked_div(
            math::checked_mul(settlement_usd as u128, 1_000_000u128)?,
            sol_price_scaled.price as u128
        )?;
        
        // Scale from 6 decimals to SOL token decimals
        if sol_custody.decimals > 6 {
            math::checked_as_u64(math::checked_mul(
                sol_amount_6_decimals,
                math::checked_pow(10u128, (sol_custody.decimals - 6) as usize)?
            )?)?
        } else {
            math::checked_as_u64(math::checked_div(
                sol_amount_6_decimals,
                math::checked_pow(10u128, (6 - sol_custody.decimals) as usize)?
            )?)?
        }
    } else {
        // Scale USDC price to 6 decimals
        let usdc_price_scaled = usdc_price.scale_to_exponent(-6)?;
        
        // USD amount / USDC price = USDC amount
        let usdc_amount_6_decimals = math::checked_div(
            math::checked_mul(settlement_usd as u128, 1_000_000u128)?,
            usdc_price_scaled.price as u128
        )?;
        
        // Scale from 6 decimals to USDC token decimals
        if usdc_custody.decimals > 6 {
            math::checked_as_u64(math::checked_mul(
                usdc_amount_6_decimals,
                math::checked_pow(10u128, (usdc_custody.decimals - 6) as usize)?
            )?)?
        } else {
            math::checked_as_u64(math::checked_div(
                usdc_amount_6_decimals,
                math::checked_pow(10u128, (6 - usdc_custody.decimals) as usize)?
            )?)?
        }
    };

    let native_exit_tokens = if position.side == Side::Long {
        // Long positions exit in SOL
        let sol_price_scaled = sol_price.scale_to_exponent(-6)?;
        let sol_amount_6_decimals = math::checked_div(
            math::checked_mul(settlement_usd as u128, 1_000_000u128)?,
            sol_price_scaled.price as u128
        )?;
        
        if sol_custody.decimals > 6 {
            math::checked_as_u64(math::checked_mul(
                sol_amount_6_decimals,
                math::checked_pow(10u128, (sol_custody.decimals - 6) as usize)?
            )?)?
        } else {
            math::checked_as_u64(math::checked_div(
                sol_amount_6_decimals,
                math::checked_pow(10u128, (6 - sol_custody.decimals) as usize)?
            )?)?
        }
    } else {
        // Short positions exit in USDC
        let usdc_price_scaled = usdc_price.scale_to_exponent(-6)?;
        let usdc_amount_6_decimals = math::checked_div(
            math::checked_mul(settlement_usd as u128, 1_000_000u128)?,
            usdc_price_scaled.price as u128
        )?;
        
        if usdc_custody.decimals > 6 {
            math::checked_as_u64(math::checked_mul(
                usdc_amount_6_decimals,
                math::checked_pow(10u128, (usdc_custody.decimals - 6) as usize)?
            )?)?
        } else {
            math::checked_as_u64(math::checked_div(
                usdc_amount_6_decimals,
                math::checked_pow(10u128, (6 - usdc_custody.decimals) as usize)?
            )?)?
        }
    };
    
    msg!("Settlement USD: {}", settlement_usd);
    msg!("Settlement tokens: {}", settlement_tokens);
    
    // Transfer settlement to user
    if settlement_tokens > 0 {
        ctx.accounts.contract.transfer_tokens(
            if params.receive_sol {
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
        math::checked_as_u64(math::checked_div(
            math::checked_mul(position.locked_amount as u128, params.close_percentage as u128)?,
            100_000_000u128
        )?)?
    };
    
    if position.side == Side::Long {
        sol_custody.token_locked = math::checked_sub(
            sol_custody.token_locked,
            locked_amount_to_release
        )?;
    } else {
        usdc_custody.token_locked = math::checked_sub(
            usdc_custody.token_locked,
            locked_amount_to_release
        )?;
    }
    
    // Update custody ownership
    if position.collateral_custody == sol_custody.key() {
        sol_custody.token_owned = math::checked_sub(
            sol_custody.token_owned,
            collateral_amount_to_close
        )?;
    } else {
        usdc_custody.token_owned = math::checked_sub(
            usdc_custody.token_owned,
            collateral_amount_to_close
        )?;
    }
    
    // Update pool open interest
    if position.side == Side::Long {
        pool.long_open_interest_usd = math::checked_sub(pool.long_open_interest_usd, size_usd_to_close as u128)?;
    } else {
        pool.short_open_interest_usd = math::checked_sub(pool.short_open_interest_usd, size_usd_to_close as u128)?;
    }
    
    // Store values before modifying position for event emission
    let position_owner = position.owner;
    let position_key = position.key();
    let position_pool = position.pool;
    
    // Update or close position
    if is_full_close {
        msg!("Position fully closed - automatically closing TP/SL orderbook and position accounts");
        
        position.is_liquidated = true; // Mark as closed
        position.size_usd = 0;
        position.collateral_amount = 0;
        position.collateral_usd = 0;
        position.locked_amount = 0;
        position.trade_fees = 0;
        
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
        
        msg!("Position fully closed - will automatically close TP/SL orderbook and position accounts");
        
    } else {
        // Update position for partial close
        position.size_usd = math::checked_sub(position.size_usd, size_usd_to_close)?;
        position.collateral_amount = math::checked_sub(position.collateral_amount, collateral_amount_to_close)?;
        position.collateral_usd = math::checked_sub(position.collateral_usd, collateral_usd_to_close)?;
        position.locked_amount = math::checked_sub(position.locked_amount, locked_amount_to_release)?;
        position.trade_fees = math::checked_sub(position.trade_fees, trade_fees_for_closed_portion)?;
    }
    
    // Update fee tracking
    position.borrow_fees_paid = math::checked_add(position.borrow_fees_paid, interest_for_closed_portion.try_into().unwrap())?;
    position.accrued_borrow_fees = math::checked_sub(position.accrued_borrow_fees, interest_for_closed_portion.try_into().unwrap())?;
    
    position.update_time = current_time;
    
    emit!(PerpPositionClosed {
        pub_key: position.key(),
        index: position.index,
        owner: position.owner,
        pool: position.pool,
        custody: position.custody,
        collateral_custody: position.collateral_custody,
        order_type: position.order_type as u8,
        side: position.side as u8,
        is_liquidated: position.is_liquidated,
        price: current_price_scaled,
        size_usd: position.size_usd,
        collateral_usd: position.collateral_usd,
        open_time: position.open_time,
        update_time: position.update_time,
        liquidation_price: position.liquidation_price,
        cumulative_interest_snapshot: position.cumulative_interest_snapshot,
        trade_fees: position.trade_fees,
        trade_fees_paid: trade_fees_for_closed_portion,
        borrow_fees_paid: interest_for_closed_portion.try_into().unwrap(),
        accrued_borrow_fees: position.accrued_borrow_fees,
        last_borrow_fees_update_time: position.last_borrow_fees_update_time,
        locked_amount: position.locked_amount,
        collateral_amount: position.collateral_amount,
        native_exit_amount: native_exit_tokens,
        trigger_price: position.trigger_price,
        trigger_above_threshold: position.trigger_above_threshold,
        bump: position.bump,
        close_percentage: params.close_percentage as u64,
        settlement_tokens: settlement_tokens,
        realized_pnl: pnl_for_closed_portion,
    });
    
    // Automatically close accounts if fully closed
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
#[instruction(params: ClosePerpPositionParams)]
pub struct ClosePerpPosition<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = owner,
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
    #[account(mut)]
    pub tp_sl_orderbook: Option<AccountInfo<'info>>,

    pub token_program: Program<'info, Token>,
}
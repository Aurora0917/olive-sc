use crate::{
    errors::{FutureError, TradingError},
    events::FutureClosed,
    math::{self, f64_to_scaled_price},
    state::{Contract, Custody, Future, FutureStatus, OraclePrice, Pool, Side},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CloseFutureParams {
    pub future_index: u64,            // Index of future to close
    pub pool_name: String,            // Pool name for seeds
    pub close_percentage: u64,        // Percentage to close (100_000_000 = 100%)
    pub receive_sol: bool,            // Settlement preference
    pub max_slippage_bps: u64,        // Maximum slippage tolerance
}

pub fn close_future(ctx: Context<CloseFuture>, params: &CloseFutureParams) -> Result<()> {
    msg!("Closing future position");
    msg!("Close percentage: {}%", params.close_percentage as f64 / 1_000_000.0);

    // Get keys first to avoid borrowing conflicts
    let sol_custody_key = ctx.accounts.sol_custody.key();
    
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let future = &mut ctx.accounts.future;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;

    // Validation
    require_keys_eq!(
        future.owner,
        ctx.accounts.owner.key(),
        TradingError::Unauthorized
    );
    require!(
        future.status == FutureStatus::Active,
        FutureError::FutureNotActive
    );
    require!(
        params.close_percentage > 0 && params.close_percentage <= 100_000_000,
        TradingError::InvalidAmount
    );

    let current_time = contract.get_time()?;
    let is_full_close = params.close_percentage == 100_000_000;

    // Check if future has expired
    if future.is_expired(current_time) {
        return Err(FutureError::FutureExpired.into());
    }

    // Get current oracle prices
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let current_sol_price = sol_price.get_price();
    let current_sol_price_scaled = f64_to_scaled_price(current_sol_price)?;

    // Calculate P&L
    let pnl = future.calculate_pnl(current_sol_price_scaled, current_time)?;
    
    msg!("Current spot price: {}", current_sol_price);
    msg!("Future price: {}", (future.future_price as f64) / 1_000_000.0);
    msg!("P&L: {}", pnl);

    // Calculate amounts to close (proportional to percentage)
    let size_usd_to_close = if is_full_close {
        future.size_usd
    } else {
        math::checked_as_u64(math::checked_div(
            math::checked_mul(future.size_usd as u128, params.close_percentage as u128)?,
            100_000_000u128
        )?)?
    };

    let collateral_amount_to_close = if is_full_close {
        future.collateral_amount
    } else {
        math::checked_as_u64(math::checked_div(
            math::checked_mul(future.collateral_amount as u128, params.close_percentage as u128)?,
            100_000_000u128
        )?)?
    };

    let collateral_usd_to_close = if is_full_close {
        future.collateral_usd
    } else {
        math::checked_as_u64(math::checked_div(
            math::checked_mul(future.collateral_usd as u128, params.close_percentage as u128)?,
            100_000_000u128
        )?)?
    };

    let locked_amount_to_release = if is_full_close {
        future.locked_amount
    } else {
        math::checked_as_u64(math::checked_div(
            math::checked_mul(future.locked_amount as u128, params.close_percentage as u128)?,
            100_000_000u128
        )?)?
    };

    // Calculate P&L for closed portion
    let pnl_for_closed_portion = if is_full_close {
        pnl
    } else {
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

    // Calculate net settlement (collateral + PnL - fees)
    let closing_fee = math::checked_div(
        math::checked_mul(size_usd_to_close as u128, Future::SETTLEMENT_FEE_BPS as u128)?,
        10_000u128,
    )? as u64;

    let net_settlement = (collateral_usd_to_close as i64) + pnl_for_closed_portion - (closing_fee as i64);
    let settlement_usd = if net_settlement > 0 { net_settlement as u64 } else { 0 };

    let native_exit_mount = if settlement_usd > 0 {
        if future.side == Side::Long {
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
        }
    } else {
        0
    };
    

    msg!("Settlement USD: {}", settlement_usd);
    msg!("Closing fee: {}", closing_fee);

    // Calculate settlement tokens
    let settlement_tokens = if settlement_usd > 0 {
        if params.receive_sol {
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
        }
    } else {
        0
    };

    // Transfer settlement to user
    if settlement_tokens > 0 {
        let settlement_token_account = if params.receive_sol {
            &ctx.accounts.sol_custody_token_account
        } else {
            &ctx.accounts.usdc_custody_token_account
        };

        contract.transfer_tokens(
            settlement_token_account.to_account_info(),
            ctx.accounts.receiving_account.to_account_info(),
            ctx.accounts.transfer_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            settlement_tokens,
        )?;

        // Update settlement custody balance
        if params.receive_sol {
            sol_custody.token_owned = math::checked_sub(
                sol_custody.token_owned,
                settlement_tokens
            )?;
        } else {
            usdc_custody.token_owned = math::checked_sub(
                usdc_custody.token_owned,
                settlement_tokens
            )?;
        }
    }

    // Release locked liquidity
    if future.side == Side::Long {
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

    // Calculate remaining collateral to return
    let remaining_collateral = if settlement_usd < collateral_usd_to_close {
        // If settlement was less than collateral, return the difference
        let diff_usd = collateral_usd_to_close - settlement_usd;
        
        // Convert back to collateral tokens
        let collateral_price_scaled = if future.collateral_custody == sol_custody_key {
            sol_price.scale_to_exponent(-6)?
        } else {
            usdc_price.scale_to_exponent(-6)?
        };
        
        let remaining_amount_6_decimals = math::checked_div(
            math::checked_mul(diff_usd as u128, 1_000_000u128)?,
            collateral_price_scaled.price as u128
        )?;
        
        let custody_decimals = if future.collateral_custody == sol_custody_key {
            sol_custody.decimals
        } else {
            usdc_custody.decimals
        };
        
        let remaining_tokens = if custody_decimals > 6 {
            math::checked_as_u64(math::checked_mul(
                remaining_amount_6_decimals,
                math::checked_pow(10u128, (custody_decimals - 6) as usize)?
            )?)?
        } else {
            math::checked_as_u64(math::checked_div(
                remaining_amount_6_decimals,
                math::checked_pow(10u128, (6 - custody_decimals) as usize)?
            )?)?
        };
        
        remaining_tokens.min(collateral_amount_to_close)
    } else {
        0
    };

    // Return remaining collateral if any
    if remaining_collateral > 0 {
        let collateral_token_account = if future.collateral_custody == sol_custody_key {
            &ctx.accounts.sol_custody_token_account
        } else {
            &ctx.accounts.usdc_custody_token_account
        };

        contract.transfer_tokens(
            collateral_token_account.to_account_info(),
            ctx.accounts.receiving_account.to_account_info(),
            ctx.accounts.transfer_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            remaining_collateral,
        )?;

        if future.collateral_custody == sol_custody_key {
            sol_custody.token_owned = math::checked_sub(
                sol_custody.token_owned,
                remaining_collateral
            )?;
        } else {
            usdc_custody.token_owned = math::checked_sub(
                usdc_custody.token_owned,
                remaining_collateral
            )?;
        }
    }

    // Update pool tracking
    let time_to_expiry_remaining = future.time_to_expiry(current_time);
    pool.remove_future_position(
        size_usd_to_close,
        time_to_expiry_remaining,
        current_time,
    )?;

    // Store values for event before updating future
    let future_key = future.key();
    let owner = future.owner;

    // Update or close future position
    if is_full_close {
        // Mark as settled for cleanup (even though closed early)
        future.status = FutureStatus::Settled;
        future.settlement_time = Some(current_time);
        future.settlement_price = Some(current_sol_price_scaled);
        future.pnl_at_settlement = Some(pnl);
        future.settlement_amount = Some(settlement_usd);
        
        // Zero out amounts
        future.size_usd = 0;
        future.collateral_usd = 0;
        future.collateral_amount = 0;
        future.locked_amount = 0;
    } else {
        // Update remaining position
        future.size_usd = math::checked_sub(future.size_usd, size_usd_to_close)?;
        future.collateral_usd = math::checked_sub(future.collateral_usd, collateral_usd_to_close)?;
        future.collateral_amount = math::checked_sub(future.collateral_amount, collateral_amount_to_close)?;
        future.locked_amount = math::checked_sub(future.locked_amount, locked_amount_to_release)?;
    }

    future.update_time = current_time;

    emit!(FutureClosed {
        owner,
        future_key,
        index: future.index,
        side: future.side as u8,
        close_percentage: params.close_percentage,
        closed_size_usd: size_usd_to_close,
        collateral_usd: future.collateral_usd,
        collateral_amount: future.collateral_amount,
        locked_amount: future.locked_amount,
        native_exit_amount: native_exit_mount,
        trade_fees: closing_fee,
        remaining_size_usd: future.size_usd,
        settlement_amount: settlement_usd,
        settlement_tokens,
        pnl: pnl_for_closed_portion,
        current_spot_price: current_sol_price_scaled,
        close_time: current_time,
    });

    msg!("Future position closed successfully");
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: CloseFutureParams)]
pub struct CloseFuture<'info> {
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
            b"future",
            owner.key().as_ref(),
            params.future_index.to_le_bytes().as_ref(),
            pool.key().as_ref()
        ],
        bump = future.bump
    )]
    pub future: Box<Account<'info, Future>>,

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

    pub token_program: Program<'info, Token>,
}
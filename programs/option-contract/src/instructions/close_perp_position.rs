use crate::{
    errors::OptionError,
    math,
    state::{Contract, Custody, OraclePrice, Pool},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use super::{PerpPosition, PerpSide};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ClosePerpPositionParams {
    pub position_index: u64,
    pub pool_name: String,
    pub close_percentage: u8,    // 1-100: 100 = full close, <100 = partial close
    pub min_price: f64,         // Slippage protection
    pub receive_sol: bool,      // true = receive SOL, false = receive USDC
}

pub fn close_perp_position(
    ctx: Context<ClosePerpPosition>,
    params: &ClosePerpPositionParams
) -> Result<()> {
    msg!("Closing {}% of perpetual position", params.close_percentage);
    
    let contract = &ctx.accounts.contract;
    let position = &mut ctx.accounts.position;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    
    // Validation
    require_keys_eq!(position.owner, ctx.accounts.owner.key(), OptionError::Unauthorized);
    require!(!position.is_liquidated, OptionError::PositionLiquidated);
    require!(
        params.close_percentage > 0 && params.close_percentage <= 100, 
        OptionError::InvalidAmount
    );
    
    // Get current prices from oracles
    let current_time = contract.get_time()?;
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let current_sol_price = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();
    let is_full_close = params.close_percentage == 100;
    
    msg!("SOL Price: {}", current_sol_price);
    msg!("USDC Price: {}", usdc_price_value);
    msg!("Closing at SOL price: ${}", current_sol_price);
    msg!("User chose to receive: {}", if params.receive_sol { "SOL" } else { "USDC" });
    
    // Slippage protection
    match position.side {
        PerpSide::Long => require!(current_sol_price >= params.min_price, OptionError::PriceSlippage),
        PerpSide::Short => require!(current_sol_price <= params.min_price, OptionError::PriceSlippage),
    }
    
    let collateral_price = if position.collateral_asset == position.sol_custody {
        current_sol_price
    } else {
        usdc_price_value
    };
    
    position.update_position(current_sol_price, current_time, collateral_price)?;
    
    // Calculate amounts to close (proportional to percentage)
    let close_ratio = params.close_percentage as f64 / 100.0;
    let position_size_to_close = if is_full_close {
        position.position_size
    } else {
        math::checked_as_u64(position.position_size as f64 * close_ratio)?
    };
    
    let collateral_amount_to_close = if is_full_close {
        position.collateral_amount
    } else {
        math::checked_as_u64(position.collateral_amount as f64 * close_ratio)?
    };
    
    // Calculate P&L for the portion being closed
    let total_pnl = position.unrealized_pnl;
    let pnl_for_closed_portion = if is_full_close {
        total_pnl
    } else {
        (total_pnl as f64 * close_ratio) as i64
    };
    
    msg!("Position size to close: {}", position_size_to_close);
    msg!("Collateral to close: {}", collateral_amount_to_close);
    msg!("P&L for closed portion: ${}", pnl_for_closed_portion as f64 / 1_000_000.0);
    
    // Calculate settlement amount in the original collateral asset
    let collateral_to_return = if pnl_for_closed_portion >= 0 {
        // Profit: return collateral + profit
        math::checked_add(collateral_amount_to_close, pnl_for_closed_portion as u64 * 1_000)?
    } else {
        // Loss: return collateral - loss (if any remaining)
        let loss = (-pnl_for_closed_portion * 1_000) as u64;
        if loss >= collateral_amount_to_close {
            0 // Total loss
        } else {
            math::checked_sub(collateral_amount_to_close, loss)?
        }
    };
    
    // Unlock locked tokens based on position side (same as before)
    match position.side {
        PerpSide::Long => {
            // Unlock SOL tokens for long position
            sol_custody.token_locked = math::checked_sub(sol_custody.token_locked, position_size_to_close)?;
        },
        PerpSide::Short => {
            // Unlock USDC equivalent for short position
            let position_value_usd = position_size_to_close as f64 / math::checked_powi(10.0, sol_custody.decimals as i32)?;
            let usdc_to_unlock = math::checked_as_u64(
                position_value_usd * math::checked_powi(10.0, usdc_custody.decimals as i32)?
            )?;
            usdc_custody.token_locked = math::checked_sub(usdc_custody.token_locked, usdc_to_unlock)?;
        }
    }
    
    // Transfer settlement funds to user (if any) - use user's preference
    if collateral_to_return > 0 {
        let authority_bump = contract.transfer_authority_bump;
        let signer_seeds: &[&[&[u8]]] = &[&[b"transfer_authority", &[authority_bump]]];
        
        let (from_account, to_account, settlement_amount) = if params.receive_sol {
            // User wants to receive SOL
            if position.collateral_asset == position.sol_custody {
                // Same asset: direct transfer
                (&ctx.accounts.sol_custody_token_account, &ctx.accounts.user_sol_account, (collateral_to_return as f64 / current_sol_price) as u64)
            } else {
                // Different asset: convert USDC to SOL equivalent
                let usdc_amount = collateral_to_return;
                let usdc_value_usd = usdc_amount as f64 / math::checked_powi(10.0, usdc_custody.decimals as i32)?;
                let sol_amount = math::checked_as_u64(
                    usdc_value_usd / current_sol_price * math::checked_powi(10.0, sol_custody.decimals as i32)?
                )?;
                (&ctx.accounts.sol_custody_token_account, &ctx.accounts.user_sol_account, sol_amount)
            }
        } else {
            // User wants to receive USDC
            if position.collateral_asset == position.usdc_custody {
                // Same asset: direct transfer
                (&ctx.accounts.usdc_custody_token_account, &ctx.accounts.user_usdc_account, (collateral_to_return as f64 / usdc_price_value) as u64)
            } else {
                // Different asset: convert SOL to USDC equivalent
                let sol_amount = collateral_to_return;
                let sol_value_usd = sol_amount as f64 / math::checked_powi(10.0, sol_custody.decimals as i32)?;
                let usdc_amount = math::checked_as_u64(
                    sol_value_usd / usdc_price_value * math::checked_powi(10.0, usdc_custody.decimals as i32)?
                )?;
                (&ctx.accounts.usdc_custody_token_account, &ctx.accounts.user_usdc_account, usdc_amount)
            }
        };
        
        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                anchor_spl::token::Transfer {
                    from: from_account.to_account_info(),
                    to: to_account.to_account_info(),
                    authority: ctx.accounts.transfer_authority.to_account_info(),
                },
                signer_seeds,
            ),
            settlement_amount,
        )?;
        
        msg!("Settlement amount transferred: {}", settlement_amount);
    }
    
    // Update custody stats (based on original collateral)
    if position.side == PerpSide::Long {
        sol_custody.token_owned = math::checked_sub(sol_custody.token_owned, collateral_amount_to_close)?;
    } else {
        usdc_custody.token_owned = math::checked_sub(usdc_custody.token_owned, collateral_amount_to_close)?;
    }
    
    if is_full_close {
        // Full close: Mark position as closed
        position.is_liquidated = true;
        msg!("Position fully closed");
    } else {
        // Partial close: Update position with remaining amounts
        position.position_size = math::checked_sub(position.position_size, position_size_to_close)?;
        position.collateral_amount = math::checked_sub(position.collateral_amount, collateral_amount_to_close)?;
        position.unrealized_pnl = total_pnl - pnl_for_closed_portion;
        
        // Recalculate metrics for remaining position
        let remaining_position_value_usd = position.position_size as f64 / math::checked_powi(10.0, sol_custody.decimals as i32)?;
        let remaining_collateral_value_usd = position.collateral_amount as f64 / math::checked_powi(10.0, if position.side == PerpSide::Long { sol_custody.decimals } else { usdc_custody.decimals } as i32)?;
        
        // Leverage stays the same in proportional close
        position.leverage = math::checked_float_div(remaining_position_value_usd, remaining_collateral_value_usd)?;
        
        // Update margin ratio
        position.last_update_time = current_time;
        
        msg!("Partial close completed:");
        msg!("Remaining position size: {}", position.position_size);
        msg!("Remaining collateral: {}", position.collateral_amount);
        msg!("Leverage: {}x", position.leverage);
        msg!("Margin ratio: {}%", position.margin_ratio * 100.0);
    }
    
    msg!("Original collateral: {}", if position.side == PerpSide::Long { "SOL" } else { "USDC" });
    msg!("Settlement received as: {}", if params.receive_sol { "SOL" } else { "USDC" });
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: ClosePerpPositionParams)]
pub struct ClosePerpPosition<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        constraint = user_sol_account.mint == sol_custody.mint,
        has_one = owner
    )]
    pub user_sol_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = user_usdc_account.mint == usdc_custody.mint,
        has_one = owner
    )]
    pub user_usdc_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: Transfer authority for custody token accounts
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
            b"perp_position",
            owner.key().as_ref(),
            params.position_index.to_le_bytes().as_ref(),
            pool.key().as_ref()
        ],
        bump = position.bump
    )]
    pub position: Box<Account<'info, PerpPosition>>,

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

    /// CHECK: SOL price oracle
    #[account(constraint = sol_oracle_account.key() == sol_custody.oracle)]
    pub sol_oracle_account: AccountInfo<'info>,

    /// CHECK: USDC price oracle
    #[account(constraint = usdc_oracle_account.key() == usdc_custody.oracle)]
    pub usdc_oracle_account: AccountInfo<'info>,

    #[account(mut)]
    pub sol_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub usdc_mint: Box<Account<'info, Mint>>,

    pub token_program: Program<'info, Token>,
}
use crate::{
    errors::OptionError,
    math,
    state::{Contract, Custody, OraclePrice, Pool},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use super::{PerpPosition, PerpSide};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AddCollateralParams {
    pub position_index: u64,
    pub pool_name: String,
    pub collateral_amount: u64,  // Amount to add as collateral (in position's collateral asset)
    pub pay_sol: bool,          // true = pay with SOL, false = pay with USDC
}

pub fn add_collateral(
    ctx: Context<AddCollateral>,
    params: &AddCollateralParams
) -> Result<()> {
    msg!("Adding collateral to perpetual position");
    
    let contract = &ctx.accounts.contract;
    let position = &mut ctx.accounts.position;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    
    // Validation
    require_keys_eq!(position.owner, ctx.accounts.owner.key(), OptionError::Unauthorized);
    require!(!position.is_liquidated, OptionError::PositionLiquidated);
    require!(params.collateral_amount > 0, OptionError::InvalidAmount);
    
    // Get current prices from oracles
    let current_time = contract.get_time()?;
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let current_sol_price = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();
    
    msg!("SOL Price: {}", current_sol_price);
    msg!("USDC Price: {}", usdc_price_value);
    
    // Determine position's collateral asset info
    let position_collateral_is_sol = position.collateral_asset == position.sol_custody;
    let (collateral_decimals, collateral_price) = if position_collateral_is_sol {
        (sol_custody.decimals, current_sol_price)
    } else {
        (usdc_custody.decimals, usdc_price_value)
    };
    
    // Determine payment asset info
    let (pay_decimals, pay_price, funding_account) = if params.pay_sol {
        (sol_custody.decimals, current_sol_price, &ctx.accounts.sol_funding_account)
    } else {
        (usdc_custody.decimals, usdc_price_value, &ctx.accounts.usdc_funding_account)
    };
    
    // Calculate payment amount needed
    let payment_amount = if params.pay_sol == true {
        math::checked_as_u64(params.collateral_amount as f64 * current_sol_price)?
    } else {
        math::checked_as_u64(params.collateral_amount as f64 * usdc_price_value * 1_000.0)?
    };
    
    msg!("Collateral amount to add: {}", params.collateral_amount);
    msg!("Payment amount required: {}", payment_amount);
    msg!("Payment asset: {}", if params.pay_sol { "SOL" } else { "USDC" });
    msg!("Position collateral asset: {}", if position_collateral_is_sol { "SOL" } else { "USDC" });
    
    // Check user has sufficient balance
    require_gte!(
        funding_account.amount,
        payment_amount,
        OptionError::InsufficientBalance
    );
    
    // Update position with current P&L first
    position.update_position(current_sol_price, current_time, collateral_price)?;
    
    msg!("Current P&L before adding collateral: ${}", position.unrealized_pnl as f64 / 1_000_000.0);
    msg!("Current margin ratio: {}%", position.margin_ratio * 100.0);
    
    // Determine source and destination for transfer
    let (from_account, to_account) = if params.pay_sol {
        if position_collateral_is_sol {
            // SOL to SOL: direct transfer
            (&ctx.accounts.sol_funding_account, &ctx.accounts.sol_custody_token_account)
        } else {
            // SOL to USDC: need to handle conversion (simplified - direct transfer for now)
            // In practice, you might want to use a DEX or oracle-based conversion
            (&ctx.accounts.sol_funding_account, &ctx.accounts.sol_custody_token_account)
        }
    } else {
        if position_collateral_is_sol {
            // USDC to SOL: need to handle conversion
            (&ctx.accounts.usdc_funding_account, &ctx.accounts.usdc_custody_token_account)
        } else {
            // USDC to USDC: direct transfer
            (&ctx.accounts.usdc_funding_account, &ctx.accounts.usdc_custody_token_account)
        }
    };
    
    // Transfer payment from user to custody
    anchor_spl::token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::Transfer {
                from: from_account.to_account_info(),
                to: to_account.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
            },
        ),
        params.collateral_amount,
    )?;
    
    // Update position collateral amount (always in position's collateral asset)
    position.collateral_amount = math::checked_add(position.collateral_amount, payment_amount)?;
    
    // Update custody stats based on which asset was actually transferred
    if params.pay_sol {
        sol_custody.token_owned = math::checked_add(sol_custody.token_owned, payment_amount)?;
        
        // If converting SOL to USDC collateral, we need to handle the conversion
        if !position_collateral_is_sol {
            // Convert SOL to USDC equivalent in custody tracking
            let sol_value_usd = payment_amount as f64 / math::checked_powi(10.0, sol_custody.decimals as i32)? * current_sol_price;
            let usdc_equivalent = math::checked_as_u64(
                sol_value_usd / usdc_price_value * math::checked_powi(10.0, usdc_custody.decimals as i32)?
            )?;
            
            // Adjust custody balances to reflect the conversion
            sol_custody.token_owned = math::checked_sub(sol_custody.token_owned, payment_amount)?;
            usdc_custody.token_owned = math::checked_add(usdc_custody.token_owned, usdc_equivalent)?;
            
            msg!("Converted {} SOL to {} USDC equivalent", payment_amount, usdc_equivalent);
        }
    } else {
        usdc_custody.token_owned = math::checked_add(usdc_custody.token_owned, payment_amount)?;
        
        // If converting USDC to SOL collateral, we need to handle the conversion
        if position_collateral_is_sol {
            // Convert USDC to SOL equivalent in custody tracking
            let usdc_value_usd = payment_amount as f64 / math::checked_powi(10.0, usdc_custody.decimals as i32)? * usdc_price_value;
            let sol_equivalent = math::checked_as_u64(
                usdc_value_usd / current_sol_price * math::checked_powi(10.0, sol_custody.decimals as i32)?
            )?;
            
            // Adjust custody balances to reflect the conversion
            usdc_custody.token_owned = math::checked_sub(usdc_custody.token_owned, payment_amount)?;
            sol_custody.token_owned = math::checked_add(sol_custody.token_owned, sol_equivalent)?;
            
            msg!("Converted {} USDC to {} SOL equivalent", payment_amount, sol_equivalent);
        }
    }
    
    // Recalculate leverage and margin ratio with new collateral
    let position_value_sol = position.position_size as f64 / math::checked_powi(10.0, sol_custody.decimals as i32)?;
    let position_value_usd = math::checked_float_mul(position_value_sol, position.entry_price)?;
    
    let collateral_value_tokens = position.collateral_amount as f64 / math::checked_powi(10.0, collateral_decimals as i32)?;
    let collateral_value_usd = math::checked_float_mul(collateral_value_tokens, collateral_price)?;
    
    // Update leverage (should decrease with more collateral)
    position.leverage = math::checked_float_div(position_value_usd, collateral_value_usd)?;
    
    // Update margin ratio (should improve with more collateral)
    let current_equity_usd = collateral_value_usd + (position.unrealized_pnl as f64 / 1_000_000.0);
    position.margin_ratio = math::checked_float_div(current_equity_usd, position_value_usd)?;
    
    // ============ LIQUIDATION PRICE UPDATE (ONLY NEW ADDITION) ============
    let maintenance_margin = PerpPosition::MAINTENANCE_MARGIN; // Usually 5% (0.05)
    let liquidation_buffer = 0.005; // 0.5% buffer for safety
    
    // Calculate the new liquidation price with updated collateral
    let is_long = position.side == PerpSide::Long;
    let entry_price = position.entry_price;
    
    let new_liquidation_price = if is_long {
        // For LONG positions: liquidation when price drops
        // At liquidation: collateral_value_usd + (liq_price - entry_price) * position_size_sol = maintenance_margin * position_value_usd
        let required_equity = position_value_usd * maintenance_margin;
        let equity_deficit = required_equity - collateral_value_usd;
        let price_change_needed = equity_deficit / position_value_sol;
        
        let liq_price = entry_price + price_change_needed - liquidation_buffer;
        f64::max(0.0, liq_price)
    } else {
        // For SHORT positions: liquidation when price rises
        // At liquidation: collateral_value_usd + (entry_price - liq_price) * position_size_sol = maintenance_margin * position_value_usd
        let required_equity = position_value_usd * maintenance_margin;
        let equity_deficit = required_equity - collateral_value_usd;
        let price_change_needed = equity_deficit / position_value_sol;
        
        entry_price - price_change_needed + liquidation_buffer
    };
    
    // Update the liquidation price
    position.liquidation_price = new_liquidation_price;
    // ============ END OF LIQUIDATION PRICE UPDATE ============
    
    position.last_update_time = current_time;
    
    // Validate new leverage is within limits
    require!(
        position.leverage <= PerpPosition::MAX_LEVERAGE && position.leverage >= 1.0,
        OptionError::InvalidLeverage
    );
    
    msg!("Collateral added successfully");
    msg!("New collateral amount: {}", position.collateral_amount);
    msg!("New leverage: {}x", position.leverage);
    msg!("New margin ratio: {}%", position.margin_ratio * 100.0);
    msg!("New liquidation price: ${}", position.liquidation_price);
    msg!("Payment asset used: {}", if params.pay_sol { "SOL" } else { "USDC" });
    msg!("Payment amount: {}", payment_amount);
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: AddCollateralParams)]
pub struct AddCollateral<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        constraint = sol_funding_account.mint == sol_custody.mint,
        has_one = owner
    )]
    pub sol_funding_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = usdc_funding_account.mint == usdc_custody.mint,
        has_one = owner
    )]
    pub usdc_funding_account: Box<Account<'info, TokenAccount>>,

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
use crate::{
    errors::OptionError,
    math,
    state::{Contract, Custody, OraclePrice, Pool},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use super::{PerpPosition, PerpSide};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RemoveCollateralParams {
    pub position_index: u64,
    pub pool_name: String,
    pub collateral_amount: u64,  // Amount to remove from collateral (in position's collateral asset)
    pub receive_sol: bool,       // true = receive SOL, false = receive USDC
}

pub fn remove_collateral(
    ctx: Context<RemoveCollateral>,
    params: &RemoveCollateralParams
) -> Result<()> {
    msg!("Removing collateral from perpetual position");
    
    let contract = &ctx.accounts.contract;
    let position = &mut ctx.accounts.position;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    
    // Validation
    require_keys_eq!(position.owner, ctx.accounts.owner.key(), OptionError::Unauthorized);
    require!(!position.is_liquidated, OptionError::PositionLiquidated);
    require!(params.collateral_amount > 0, OptionError::InvalidAmount);
    require!(
        params.collateral_amount < position.collateral_amount,
        OptionError::InvalidAmount
    );
    
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
    
    // Update position with current P&L first
    position.update_position(current_sol_price, current_time, collateral_price)?;
    
    msg!("Current P&L before removing collateral: ${}", position.unrealized_pnl as f64 / 1_000_000.0);
    msg!("Current margin ratio: {}%", position.margin_ratio * 100.0);

    // Calculate withdrawal amount in desired asset
    let withdrawal_amount = if params.receive_sol == true {
        // Same asset: direct withdrawal
        math::checked_as_u64(params.collateral_amount as f64 * current_sol_price)?
    } else {
        math::checked_as_u64(params.collateral_amount as f64 * usdc_price_value * 1_000.0)?
    };
    
    // Calculate new position metrics after collateral removal
    let new_collateral_amount = math::checked_sub(position.collateral_amount, withdrawal_amount)?;
    
    let position_value_sol = position.position_size as f64 / math::checked_powi(10.0, sol_custody.decimals as i32)?;
    let position_value_usd = math::checked_float_mul(position_value_sol, position.entry_price)?;
    
    let new_collateral_value_tokens = new_collateral_amount as f64 / math::checked_powi(10.0, collateral_decimals as i32)?;
    let new_collateral_value_usd = math::checked_float_mul(new_collateral_value_tokens, collateral_price)?;
    
    // Calculate new leverage and margin ratio
    let new_leverage = math::checked_float_div(position_value_usd, new_collateral_value_usd)?;
    let new_equity_usd = new_collateral_value_usd + (position.unrealized_pnl as f64 / 1_000_000.0);
    let new_margin_ratio = math::checked_float_div(new_equity_usd, position_value_usd)?;
    
    // Validate new leverage and margin ratio are within safe limits
    require!(
        new_leverage <= PerpPosition::MAX_LEVERAGE,
        OptionError::InvalidLeverage
    );
    require!(
        new_margin_ratio >= PerpPosition::MAINTENANCE_MARGIN,
        OptionError::InsufficientCollateral
    );
    
    msg!("New leverage after removal: {}x", new_leverage);
    msg!("New margin ratio after removal: {}%", new_margin_ratio * 100.0);
    
    // ============ LIQUIDATION PRICE UPDATE (ONLY NEW ADDITION) ============
    let maintenance_margin = PerpPosition::MAINTENANCE_MARGIN; // Usually 5% (0.05)
    let liquidation_buffer = 0.005; // 0.5% buffer for safety
    
    // Calculate the new liquidation price with reduced collateral
    let is_long = position.side == PerpSide::Long;
    let entry_price = position.entry_price;
    
    let new_liquidation_price = if is_long {
        // For LONG positions: liquidation when price drops
        // At liquidation: new_collateral_value_usd + (liq_price - entry_price) * position_size_sol = maintenance_margin * position_value_usd
        let required_equity = position_value_usd * maintenance_margin;
        let equity_deficit = required_equity - new_collateral_value_usd;
        let price_change_needed = equity_deficit / position_value_sol;
        
        let liq_price = entry_price + price_change_needed - liquidation_buffer;
        f64::max(0.0, liq_price)
    } else {
        // For SHORT positions: liquidation when price rises
        // At liquidation: new_collateral_value_usd + (entry_price - liq_price) * position_size_sol = maintenance_margin * position_value_usd
        let required_equity = position_value_usd * maintenance_margin;
        let equity_deficit = required_equity - new_collateral_value_usd;
        let price_change_needed = equity_deficit / position_value_sol;
        
        entry_price - price_change_needed + liquidation_buffer
    };
    // ============ END OF LIQUIDATION PRICE UPDATE ============
    
    // Determine withdrawal accounts
    let (from_account, to_account) = if params.receive_sol {
        (&ctx.accounts.sol_custody_token_account, &ctx.accounts.user_sol_account)
    } else {
        (&ctx.accounts.usdc_custody_token_account, &ctx.accounts.user_usdc_account)
    };
    
    msg!("Collateral amount to remove: {}", params.collateral_amount);
    msg!("Withdrawal amount: {}", withdrawal_amount);
    msg!("Receive asset: {}", if params.receive_sol { "SOL" } else { "USDC" });
    
    // Transfer collateral to user
    let authority_bump = contract.transfer_authority_bump;
    let signer_seeds: &[&[&[u8]]] = &[&[b"transfer_authority", &[authority_bump]]];
    
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
        params.collateral_amount,
    )?;
    
    // Update position with new collateral amount and metrics
    position.collateral_amount = new_collateral_amount;
    position.leverage = new_leverage;
    position.margin_ratio = new_margin_ratio;
    position.liquidation_price = new_liquidation_price; // Update liquidation price
    position.last_update_time = current_time;
    
    // Update custody stats based on withdrawal asset
    if params.receive_sol {
        sol_custody.token_owned = math::checked_sub(sol_custody.token_owned, withdrawal_amount)?;
        
        // If converting from USDC collateral to SOL withdrawal, handle conversion tracking
        if !position_collateral_is_sol {
            // Add back the USDC equivalent that was conceptually converted
            let collateral_value_usd = params.collateral_amount as f64 / math::checked_powi(10.0, usdc_custody.decimals as i32)? * usdc_price_value;
            let usdc_equivalent = math::checked_as_u64(
                collateral_value_usd / usdc_price_value * math::checked_powi(10.0, usdc_custody.decimals as i32)?
            )?;
            usdc_custody.token_owned = math::checked_add(usdc_custody.token_owned, usdc_equivalent)?;
            
            msg!("Converted {} USDC collateral to {} SOL withdrawal", params.collateral_amount, withdrawal_amount);
        }
    } else {
        usdc_custody.token_owned = math::checked_sub(usdc_custody.token_owned, withdrawal_amount)?;
        
        // If converting from SOL collateral to USDC withdrawal, handle conversion tracking
        if position_collateral_is_sol {
            // Add back the SOL equivalent that was conceptually converted
            let collateral_value_usd = params.collateral_amount as f64 / math::checked_powi(10.0, sol_custody.decimals as i32)? * current_sol_price;
            let sol_equivalent = math::checked_as_u64(
                collateral_value_usd / current_sol_price * math::checked_powi(10.0, sol_custody.decimals as i32)?
            )?;
            sol_custody.token_owned = math::checked_add(sol_custody.token_owned, sol_equivalent)?;
            
            msg!("Converted {} SOL collateral to {} USDC withdrawal", params.collateral_amount, withdrawal_amount);
        }
    }
    
    msg!("Collateral removed successfully");
    msg!("Removed amount: {}", params.collateral_amount);
    msg!("Withdrawal amount: {}", withdrawal_amount);
    msg!("New collateral amount: {}", position.collateral_amount);
    msg!("New leverage: {}x", position.leverage);
    msg!("New margin ratio: {}%", position.margin_ratio * 100.0);
    msg!("New liquidation price: ${}", position.liquidation_price); // Added log for new liquidation price
    msg!("Position collateral asset: {}", if position_collateral_is_sol { "SOL" } else { "USDC" });
    msg!("Received as: {}", if params.receive_sol { "SOL" } else { "USDC" });
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: RemoveCollateralParams)]
pub struct RemoveCollateral<'info> {
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
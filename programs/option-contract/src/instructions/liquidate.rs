// ==================== instructions/liquidate.rs ====================
use crate::{
    errors::OptionError,
    math,
    state::{Contract, Custody, OraclePrice, Pool},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use super::{PerpPosition, PerpSide};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct LiquidateParams {
    pub position_index: u64,
    pub pool_name: String,
}

pub fn liquidate(
    ctx: Context<Liquidate>,
    params: &LiquidateParams
) -> Result<()> {
    msg!("Liquidating perpetual position");
    
    let contract = &ctx.accounts.contract;
    let position = &mut ctx.accounts.position;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    
    // Check permissions - anyone can liquidate an eligible position
    require!(!position.is_liquidated, OptionError::PositionLiquidated);
    
    // Get current prices from oracles
    let current_time = contract.get_time()?;
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let current_sol_price = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();
    
    msg!("SOL Price: {}", current_sol_price);
    msg!("USDC Price: {}", usdc_price_value);
    msg!("Liquidating position owned by: {}", position.owner);
    
    // Determine collateral asset info
    let (collateral_price, collateral_decimals) = if position.collateral_asset == position.sol_custody {
        (current_sol_price, sol_custody.decimals)
    } else {
        (usdc_price_value, usdc_custody.decimals)
    };
    
    // Update position with current P&L and margin ratio
    position.update_position(current_sol_price, current_time, collateral_price)?;
    
    msg!("Position margin ratio: {}%", position.margin_ratio * 100.0);
    msg!("Liquidation threshold: {}%", PerpPosition::LIQUIDATION_THRESHOLD * 100.0);
    
    // Check if position can be liquidated (margin ratio below threshold)
    require!(
        position.margin_ratio <= PerpPosition::LIQUIDATION_THRESHOLD,
        OptionError::PositionNotLiquidatable
    );
    
    msg!("Position is eligible for liquidation");
    
    // Calculate liquidation amounts
    let total_pnl = position.unrealized_pnl;
    let collateral_value_tokens = position.collateral_amount as f64 / math::checked_powi(10.0, collateral_decimals)?;
    let collateral_value_usd = math::checked_float_mul(collateral_value_tokens, collateral_price)?;
    
    // Calculate settlement after P&L
    let settlement_value_usd = collateral_value_usd + (total_pnl as f64 / 1_000_000.0);
    let settlement_amount_tokens = if settlement_value_usd > 0.0 {
        math::checked_as_u64(settlement_value_usd / collateral_price * math::checked_powi(10.0, collateral_decimals)?)?
    } else {
        0 // Total loss
    };
    
    // Calculate liquidation reward (e.g., 5% of remaining collateral or minimum amount)
    let liquidation_reward_rate = 0.05; // 5% liquidation reward
    let min_liquidation_reward = 1000; // Minimum reward in tokens
    
    let liquidation_reward = if settlement_amount_tokens > 0 {
        let calculated_reward = math::checked_as_u64(settlement_amount_tokens as f64 * liquidation_reward_rate)?;
        calculated_reward.max(min_liquidation_reward).min(settlement_amount_tokens)
    } else {
        0
    };
    
    let user_settlement = if settlement_amount_tokens > liquidation_reward {
        math::checked_sub(settlement_amount_tokens, liquidation_reward)?
    } else {
        0
    };
    
    msg!("Settlement amount: {}", settlement_amount_tokens);
    msg!("Liquidation reward: {}", liquidation_reward);
    msg!("User receives: {}", user_settlement);
    
    // Unlock locked tokens based on position side
    match position.side {
        PerpSide::Long => {
            sol_custody.token_locked = math::checked_sub(sol_custody.token_locked, position.position_size)?;
        },
        PerpSide::Short => {
            let position_value_sol = position.position_size as f64 / math::checked_powi(10.0, sol_custody.decimals as i32)?;
            let position_value_usd = math::checked_float_mul(position_value_sol, position.entry_price)?;
            let usdc_to_unlock = math::checked_as_u64(
                position_value_usd * math::checked_powi(10.0, usdc_custody.decimals as i32)?
            )?;
            usdc_custody.token_locked = math::checked_sub(usdc_custody.token_locked, usdc_to_unlock)?;
        }
    }
    
    let authority_bump = contract.transfer_authority_bump;
    let signer_seeds: &[&[&[u8]]] = &[&[b"transfer_authority", &[authority_bump]]];
    
    // Determine which custody accounts to use based on collateral asset
    let (custody_token_account, user_account, liquidator_account) = if position.collateral_asset == position.sol_custody {
        (&ctx.accounts.sol_custody_token_account, &ctx.accounts.user_sol_account, &ctx.accounts.liquidator_sol_account)
    } else {
        (&ctx.accounts.usdc_custody_token_account, &ctx.accounts.user_usdc_account, &ctx.accounts.liquidator_usdc_account)
    };
    
    // Transfer remaining collateral to position owner (if any)
    if user_settlement > 0 {
        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                anchor_spl::token::Transfer {
                    from: custody_token_account.to_account_info(),
                    to: user_account.to_account_info(),
                    authority: ctx.accounts.transfer_authority.to_account_info(),
                },
                signer_seeds,
            ),
            user_settlement,
        )?;
        msg!("Transferred {} tokens to position owner", user_settlement);
    }
    
    // Transfer liquidation reward to liquidator
    if liquidation_reward > 0 {
        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                anchor_spl::token::Transfer {
                    from: custody_token_account.to_account_info(),
                    to: liquidator_account.to_account_info(),
                    authority: ctx.accounts.transfer_authority.to_account_info(),
                },
                signer_seeds,
            ),
            liquidation_reward,
        )?;
        msg!("Transferred {} tokens to liquidator as reward", liquidation_reward);
    }
    
    // Update custody stats - remove all collateral from the system
    if position.collateral_asset == position.sol_custody {
        sol_custody.token_owned = math::checked_sub(sol_custody.token_owned, position.collateral_amount)?;
    } else {
        usdc_custody.token_owned = math::checked_sub(usdc_custody.token_owned, position.collateral_amount)?;
    }
    
    // Mark position as liquidated
    position.is_liquidated = true;
    position.last_update_time = current_time;
    
    msg!("Position liquidated successfully");
    msg!("Position side: {}", if position.side == PerpSide::Long { "Long" } else { "Short" });
    msg!("Collateral asset: {}", if position.collateral_asset == position.sol_custody { "SOL" } else { "USDC" });
    msg!("Liquidator: {}", ctx.accounts.liquidator.key());
    msg!("Final P&L: ${}", total_pnl as f64 / 1_000_000.0);
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: LiquidateParams)]
pub struct Liquidate<'info> {
    #[account(mut)]
    pub liquidator: Signer<'info>,

    // Position owner's receiving accounts
    #[account(
        mut,
        constraint = user_sol_account.mint == sol_custody.mint,
        constraint = user_sol_account.owner == position.owner
    )]
    pub user_sol_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = user_usdc_account.mint == usdc_custody.mint,
        constraint = user_usdc_account.owner == position.owner
    )]
    pub user_usdc_account: Box<Account<'info, TokenAccount>>,

    // Liquidator's reward receiving accounts
    #[account(
        mut,
        constraint = liquidator_sol_account.mint == sol_custody.mint,
        constraint = liquidator_sol_account.owner == liquidator.key()
    )]
    pub liquidator_sol_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = liquidator_usdc_account.mint == usdc_custody.mint,
        constraint = liquidator_usdc_account.owner == liquidator.key()
    )]
    pub liquidator_usdc_account: Box<Account<'info, TokenAccount>>,

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
            position.owner.as_ref(),
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
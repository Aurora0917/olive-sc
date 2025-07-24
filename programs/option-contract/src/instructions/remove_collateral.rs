use crate::{
    errors::{PerpetualError, TradingError},
    events::CollateralRemoved,
    math::{self, f64_to_scaled_price},
    utils::risk_management::*,
    state::{Contract, Custody, OraclePrice, Pool, Position, Side, OrderType},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RemoveCollateralParams {
    pub position_index: u64,
    pub pool_name: String,
    pub collateral_amount: u64,  // Amount to remove from collateral
    pub receive_sol: bool,       // true = receive SOL, false = receive USDC
}

pub fn remove_collateral(
    ctx: Context<RemoveCollateral>,
    params: &RemoveCollateralParams
) -> Result<()> {
    msg!("Removing collateral from perpetual position");
    
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let position = &mut ctx.accounts.position;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    
    // Validation
    require_keys_eq!(position.owner, ctx.accounts.owner.key(), TradingError::Unauthorized);
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require!(position.order_type == OrderType::Market, PerpetualError::InvalidOrderType);
    require!(params.collateral_amount > 0, TradingError::InvalidAmount);
    require!(
        params.collateral_amount < position.collateral_amount,
        TradingError::InvalidAmount
    );
    
    // Get current prices
    let current_time = contract.get_time()?;
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let sol_price_value = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();
    let current_price_scaled = f64_to_scaled_price(sol_price_value)?;
    
    msg!("SOL Price: {}", sol_price_value);
    msg!("USDC Price: {}", usdc_price_value);
    msg!("Removing {} tokens from collateral", params.collateral_amount);
    
    // Calculate USD value to remove based on what the user wants to withdraw
    // params.collateral_amount is in the asset user wants to receive (receive_sol)
    let collateral_usd_to_remove = if params.receive_sol {
        // User wants to receive SOL, so params.collateral_amount is in SOL
        math::checked_as_u64(math::checked_float_mul(
            params.collateral_amount as f64 / math::checked_powi(10.0, sol_custody.decimals as i32)?,
            sol_price_value
        )? * 1_000_000.0)?
    } else {
        // User wants to receive USDC, so params.collateral_amount is in USDC
        math::checked_as_u64(math::checked_float_mul(
            params.collateral_amount as f64 / math::checked_powi(10.0, usdc_custody.decimals as i32)?,
            usdc_price_value
        )? * 1_000_000.0)?
    };
    
    msg!("Collateral USD to remove: {}", collateral_usd_to_remove);
    msg!("Current collateral USD: {}", position.collateral_usd);
    
    // Calculate new collateral USD
    let new_collateral_usd = math::checked_sub(position.collateral_usd, collateral_usd_to_remove)?;
    
    // Calculate new leverage and ensure it doesn't exceed limits
    let new_leverage = math::checked_float_div(position.size_usd as f64, position.collateral_usd as f64)?.max(1.0);
    require!(new_leverage <= Position::MAX_LEVERAGE, PerpetualError::InvalidLeverage);
    
    // Calculate new margin requirements
    let new_initial_margin_bps = math::checked_as_u64(math::checked_float_div(10_000.0, new_leverage)?)?; // 10000 / leverage
    
    // Ensure new margin requirements meet minimum standards
    require!(
        new_initial_margin_bps >= Position::MIN_INITIAL_MARGIN_BPS,
        PerpetualError::InvalidLeverage
    );
    
    // Check if position would be liquidatable after removing collateral
    let new_liquidation_price = calculate_liquidation_price(
        position.price,
        new_leverage,
        position.side
    )?;
    
    // Ensure position won't be immediately liquidatable
    let would_be_liquidatable = match position.side {
        Side::Long => current_price_scaled <= new_liquidation_price,
        Side::Short => current_price_scaled >= new_liquidation_price,
    };
    
    require!(!would_be_liquidatable, PerpetualError::WouldCauseLiquidation);
    
    // Check margin ratio wouldn't be too low
    let pnl = position.calculate_pnl(current_price_scaled)?;
    let new_equity = if pnl >= 0 {
        new_collateral_usd + pnl as u64
    } else {
        let loss = (-pnl) as u64;
        if loss >= new_collateral_usd {
            0
        } else {
            new_collateral_usd - loss
        }
    };
    
    let new_margin_ratio_bps = math::checked_as_u64(math::checked_div(
        math::checked_mul(new_equity as u128, 10_000u128)?,
        position.size_usd as u128,
    )?)?;
    
    require!(
        new_margin_ratio_bps > Position::LIQUIDATION_MARGIN_BPS + 20, // 1% buffer
        PerpetualError::InsufficientMargin
    );
    
    // The withdrawal amount is exactly what the user requested
    let withdrawal_tokens = params.collateral_amount;
    
    msg!("Withdrawal tokens: {}", withdrawal_tokens);
    
    // Check if custody has enough tokens for withdrawal
    if params.receive_sol {
        require_gte!(
            sol_custody.token_owned,
            withdrawal_tokens,
            TradingError::InsufficientBalance
        );
    } else {
        require_gte!(
            usdc_custody.token_owned,
            withdrawal_tokens,
            TradingError::InsufficientBalance
        );
    }
    
    // Transfer withdrawal to user
    if withdrawal_tokens > 0 {
        ctx.accounts.contract.transfer_tokens(
            if params.receive_sol {
                ctx.accounts.sol_custody_token_account.to_account_info()
            } else {
                ctx.accounts.usdc_custody_token_account.to_account_info()
            },
            ctx.accounts.receiving_account.to_account_info(),
            ctx.accounts.transfer_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            withdrawal_tokens,
        )?;
    }
    
    // Update custody stats based on where tokens are withdrawn from
    // This accounts for cross-asset withdrawals (e.g., withdrawing SOL from USDC collateral)
    if params.receive_sol {
        sol_custody.token_owned = math::checked_sub(
            sol_custody.token_owned,
            withdrawal_tokens
        )?;
    } else {
        usdc_custody.token_owned = math::checked_sub(
            usdc_custody.token_owned,
            withdrawal_tokens
        )?;
    }
    
    // Update position
    // Calculate how much to subtract from the stored collateral amount
    let collateral_amount_to_subtract = if position.collateral_custody == sol_custody.key() {
        // Position stores SOL
        if params.receive_sol {
            // Withdrawing SOL from SOL position - direct subtract
            params.collateral_amount
        } else {
            // Withdrawing USDC from SOL position - convert USDC to SOL equivalent
            let usd_value = collateral_usd_to_remove as f64 / 1_000_000.0;
            let sol_value = usd_value / sol_price_value;
            math::checked_as_u64(sol_value * math::checked_powi(10.0, sol_custody.decimals as i32)?)?
        }
    } else {
        // Position stores USDC
        if params.receive_sol {
            // Withdrawing SOL from USDC position - convert SOL to USDC equivalent
            let usd_value = collateral_usd_to_remove as f64 / 1_000_000.0;
            let usdc_value = usd_value / usdc_price_value;
            math::checked_as_u64(usdc_value * math::checked_powi(10.0, usdc_custody.decimals as i32)?)?
        } else {
            // Withdrawing USDC from USDC position - direct subtract
            params.collateral_amount
        }
    };
    
    position.collateral_amount = math::checked_sub(
        position.collateral_amount,
        collateral_amount_to_subtract
    )?;
    position.collateral_usd = new_collateral_usd;
    // Update accrued borrow fees before modifying position
    pool.update_position_borrow_fees(position, current_time, sol_custody, usdc_custody)?;
    
    position.liquidation_price = new_liquidation_price;
    position.update_time = current_time;
    
    msg!("Successfully removed collateral");
    msg!("New collateral amount: {}", position.collateral_amount);
    msg!("New collateral USD: {}", position.collateral_usd);
    msg!("New leverage: {}x", new_leverage);
    msg!("New liquidation price: {}", position.liquidation_price);
    msg!("Withdrawal amount: {} tokens", withdrawal_tokens);
    
    emit!(CollateralRemoved {
        pub_key: position.key(),
        owner: ctx.accounts.owner.key(),
        position_index: params.position_index,
        pool: pool.key(),
        custody: position.custody,
        collateral_custody: position.collateral_custody,
        order_type: position.order_type as u8,
        side: position.side as u8,
        collateral_amount_removed: params.collateral_amount,
        collateral_usd_removed: collateral_usd_to_remove,
        new_collateral_amount: position.collateral_amount,
        new_collateral_usd: position.collateral_usd,
        new_leverage,
        new_liquidation_price: position.liquidation_price,
        withdrawal_tokens,
        withdrawal_asset: if params.receive_sol { sol_custody.mint } else { usdc_custody.mint },
        update_time: current_time,
        accrued_borrow_fees: position.accrued_borrow_fees,
        last_borrow_fees_update_time: position.last_borrow_fees_update_time,
    });
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: RemoveCollateralParams)]
pub struct RemoveCollateral<'info> {
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
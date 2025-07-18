use crate::{
    errors::{PerpetualError, TradingError},
    events::PerpPositionClosed,
    math::{self, f64_to_scaled_price},
    state::{Contract, Custody, OraclePrice, Pool, Position, Side, PositionType},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ClosePerpPositionParams {
    pub position_index: u64,
    pub pool_name: String,
    pub close_percentage: u8,        // 1-100: 100 = full close, <100 = partial close
    pub min_price: f64,             // Slippage protection
    pub receive_sol: bool,          // true = receive SOL, false = receive USDC
}

pub fn close_perp_position(
    ctx: Context<ClosePerpPosition>,
    params: &ClosePerpPositionParams
) -> Result<()> {
    msg!("Closing {}% of perpetual position", params.close_percentage);
    
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let position = &mut ctx.accounts.position;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    
    // Update pool rates using the borrow rate curve
    let current_time = Clock::get()?.unix_timestamp;
    
    // Get custody accounts for utilization calculation
    let custodies_slice = [sol_custody.as_ref(), usdc_custody.as_ref()];
    let custodies_vec: Vec<Custody> = custodies_slice.iter().map(|c| (***c).clone()).collect();
    pool.update_rates(current_time, &custodies_vec)?;
    
    // Validation
    require_keys_eq!(position.owner, ctx.accounts.owner.key(), TradingError::Unauthorized);
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require!(position.position_type == PositionType::Market, PerpetualError::InvalidPositionType);
    require!(
        params.close_percentage > 0 && params.close_percentage <= 100, 
        TradingError::InvalidAmount
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
    msg!("Position side {:?}",  position.side);
    msg!("min price {}",  params.min_price);
    
    // Slippage protection
    let current_price_scaled = f64_to_scaled_price(current_sol_price)?;
    let min_price_scaled = f64_to_scaled_price(params.min_price)?;
    
    match position.side {
        Side::Long => require!(current_price_scaled >= min_price_scaled, TradingError::PriceSlippage),
        Side::Short => require!(current_price_scaled <= min_price_scaled, TradingError::PriceSlippage),
    }
    
    // Calculate P&L
    let pnl = position.calculate_pnl(current_price_scaled)?;
    
    // Calculate funding and interest payments
    let funding_payment = pool.get_funding_payment(
        position.side == Side::Long,
        position.size_usd as u128,
        position.cumulative_funding_snapshot.try_into().unwrap()
    )?;
    let interest_payment = pool.get_interest_payment(
        position.borrow_size_usd as u128,
        position.cumulative_interest_snapshot
    )?;
    
    // Calculate amounts to close (proportional to percentage)
    let close_ratio = params.close_percentage as f64 / 100.0;
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
    
    // Calculate P&L, funding, and interest for the portion being closed
    let pnl_for_closed_portion = if is_full_close {
        pnl
    } else {
        (pnl as f64 * close_ratio) as i64
    };
    
    let funding_for_closed_portion = if is_full_close {
        funding_payment
    } else {
        ((funding_payment as f64 * close_ratio) as i64).into()
    };
    
    let interest_for_closed_portion = if is_full_close {
        interest_payment
    } else {
        math::checked_as_u64(interest_payment as f64 * close_ratio)?.into()
    };
    
    msg!("Size USD to close: {}", size_usd_to_close);
    msg!("Collateral amount to close: {}", collateral_amount_to_close);
    msg!("P&L for closed portion: {}", pnl_for_closed_portion);
    msg!("Funding for closed portion: {}", funding_for_closed_portion);
    msg!("Interest for closed portion: {}", interest_for_closed_portion);
    
    // Calculate net settlement amount
    let mut net_settlement = collateral_usd_to_close as i64 + pnl_for_closed_portion - funding_for_closed_portion as i64 - interest_for_closed_portion as i64;
    
    // Ensure settlement is not negative
    if net_settlement < 0 {
        net_settlement = 0;
    }
    
    let settlement_usd = net_settlement as u64;
    
    // Calculate settlement amount in requested asset
    let (settlement_amount, settlement_decimals) = if params.receive_sol {
        let amount = math::checked_as_u64(settlement_usd as f64 / current_sol_price)?;
        (amount, sol_custody.decimals)
    } else {
        let amount = math::checked_as_u64(settlement_usd as f64 / usdc_price_value)?;
        (amount, usdc_custody.decimals)
    };
    
    // Adjust for token decimals
    let settlement_tokens = math::checked_as_u64(
        settlement_amount as f64 * math::checked_powi(10.0, settlement_decimals as i32)? / 1_000_000.0
    )?;
    
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
        math::checked_as_u64(position.locked_amount as f64 * close_ratio)?
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
    
    // Update or close position
    if is_full_close {
        position.is_liquidated = true; // Mark as closed
        position.size_usd = 0;
        position.collateral_amount = 0;
        position.collateral_usd = 0;
        position.locked_amount = 0;
    } else {
        // Update position for partial close
        position.size_usd = math::checked_sub(position.size_usd, size_usd_to_close)?;
        position.collateral_amount = math::checked_sub(position.collateral_amount, collateral_amount_to_close)?;
        position.collateral_usd = math::checked_sub(position.collateral_usd, collateral_usd_to_close)?;
        position.locked_amount = math::checked_sub(position.locked_amount, locked_amount_to_release)?;
        position.borrow_size_usd = position.size_usd.saturating_sub(position.collateral_usd);
    }
    
    // Update fee tracking
    let closing_fee = math::checked_div(size_usd_to_close, 1000)?; // 0.1% closing fee
    position.total_fees_paid = math::checked_add(position.total_fees_paid, closing_fee)?;
    position.total_fees_paid = math::checked_add(position.total_fees_paid, interest_for_closed_portion.try_into().unwrap())?;
    
    position.update_time = current_time;
    
    emit!(PerpPositionClosed {
        owner: position.owner,
        pool: position.pool,
        custody: position.custody,
        collateral_custody: position.collateral_custody,
        position_type: position.position_type as u8,
        side: position.side as u8,
        is_liquidated: position.is_liquidated,
        price: position.price,
        size_usd: position.size_usd,
        borrow_size_usd: position.borrow_size_usd,
        collateral_usd: position.collateral_usd,
        open_time: position.open_time,
        update_time: position.update_time,
        liquidation_price: position.liquidation_price,
        cumulative_interest_snapshot: position.cumulative_interest_snapshot,
        cumulative_funding_snapshot: position.cumulative_funding_snapshot,
        opening_fee_paid: position.opening_fee_paid,
        total_fees_paid: position.total_fees_paid,
        locked_amount: position.locked_amount,
        collateral_amount: position.collateral_amount,
        take_profit_price: position.take_profit_price,
        stop_loss_price: position.stop_loss_price,
        trigger_price: position.trigger_price,
        trigger_above_threshold: position.trigger_above_threshold,
        bump: position.bump,
        close_percentage: params.close_percentage as u64,
        settlement_tokens: settlement_tokens,
    });
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: ClosePerpPositionParams)]
pub struct ClosePerpPosition<'info> {
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
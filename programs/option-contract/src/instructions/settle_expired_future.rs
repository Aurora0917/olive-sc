use crate::{
    errors::{FutureError, TradingError},
    events::FutureSettled,
    math::{self, f64_to_scaled_price},
    state::{Contract, Custody, Future, FutureStatus, OraclePrice, Pool, Side},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SettleExpiredFutureParams {
    pub future_index: u64,         // Index of future to settle
    pub pool_name: String,         // Pool name for seeds
    pub owner: Pubkey,             // Owner of the future (for seeds)
}

pub fn settle_expired_future(ctx: Context<SettleExpiredFuture>, params: &SettleExpiredFutureParams) -> Result<()> {
    msg!("Settling expired future position");

    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let future = &mut ctx.accounts.future;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;

    // Validation
    require_keys_eq!(
        future.owner,
        params.owner,
        TradingError::Unauthorized
    );

    let current_time = contract.get_time()?;

    // Check if future has expired
    require!(
        future.is_expired(current_time),
        FutureError::FutureNotYetExpired
    );
    
    // Future must be active to settle
    require!(
        future.status == FutureStatus::Active,
        FutureError::FutureNotActive
    );

    // Get final settlement price from oracle
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let settlement_spot_price = sol_price.get_price();
    let settlement_spot_price_scaled = f64_to_scaled_price(settlement_spot_price)?;

    msg!("Settlement spot price: {}", settlement_spot_price);
    msg!("Original future price: {}", (future.future_price as f64) / 1_000_000.0);

    // Mark future as expired first
    future.mark_expired(current_time)?;

    // Calculate settlement
    let settlement_amount = future.settle_future(settlement_spot_price_scaled, current_time)?;
    let pnl = future.pnl_at_settlement.unwrap();

    msg!("Settlement amount USD: {}", settlement_amount);
    msg!("Final P&L: {}", pnl);

    // Convert settlement to tokens for transfer
    let settlement_tokens = if settlement_amount > 0 {
        // Always settle in the same asset as collateral was provided
        if future.collateral_custody == sol_custody.key() {
            // Settle in SOL
            let sol_price_scaled = sol_price.scale_to_exponent(-6)?;
            let sol_amount_6_decimals = math::checked_div(
                math::checked_mul(settlement_amount as u128, 1_000_000u128)?,
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
            // Settle in USDC
            let usdc_price_scaled = usdc_price.scale_to_exponent(-6)?;
            let usdc_amount_6_decimals = math::checked_div(
                math::checked_mul(settlement_amount as u128, 1_000_000u128)?,
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

    // Transfer settlement to owner if any
    if settlement_tokens > 0 {
        let settlement_token_account = if future.collateral_custody == sol_custody.key() {
            &ctx.accounts.sol_custody_token_account
        } else {
            &ctx.accounts.usdc_custody_token_account
        };

        contract.transfer_tokens(
            settlement_token_account.to_account_info(),
            ctx.accounts.owner_token_account.to_account_info(),
            ctx.accounts.transfer_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            settlement_tokens,
        )?;

        // Update custody balance
        if future.collateral_custody == sol_custody.key() {
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
            future.locked_amount
        )?;
    } else {
        usdc_custody.token_locked = math::checked_sub(
            usdc_custody.token_locked,
            future.locked_amount
        )?;
    }

    // Update pool tracking
    let time_remaining = 0; // Future has expired
    pool.remove_future_position(
        future.size_usd,
        time_remaining,
        current_time,
    )?;

    // Store values for events
    let future_key = future.key();
    let owner = future.owner;
    let size_usd = future.size_usd;

    emit!(FutureSettled {
        owner,
        future_key,
        index: future.index,
        side: future.side as u8,
        size_usd,
        settlement_price: settlement_spot_price_scaled,
        original_future_price: future.future_price,
        pnl,
        settlement_amount,
        settlement_tokens,
        expiry_time: future.expiry_time,
        settlement_time: current_time,
    });

    msg!("Future position settled successfully");
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: SettleExpiredFutureParams)]
pub struct SettleExpiredFuture<'info> {
    /// CHECK: This can be any account - keeper, owner, or other authorized party
    #[account(mut)]
    pub settler: Signer<'info>,

    /// CHECK: Owner of the future position (for receiving settlement)
    #[account(mut)]
    pub owner_token_account: Box<Account<'info, TokenAccount>>,

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
            params.owner.as_ref(),
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
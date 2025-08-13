use crate::{
    errors::{FutureError, TradingError},
    events::{FutureClaimed, FutureAccountClosed},
    math,
    state::{Contract, Custody, Future, FutureStatus, OraclePrice, Pool},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ClaimFutureParams {
    pub future_index: u64,         // Index of future to claim
    pub pool_name: String,         // Pool name for seeds
    pub close_account: bool,       // Whether to close the future account after claiming
}

pub fn claim_future(ctx: Context<ClaimFuture>, params: &ClaimFutureParams) -> Result<()> {
    msg!("Claiming settled future position");

    let contract = &ctx.accounts.contract;
    let future = &mut ctx.accounts.future;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;

    // Validation
    require_keys_eq!(
        future.owner,
        ctx.accounts.owner.key(),
        TradingError::Unauthorized
    );

    // Future must be settled or liquidated to claim
    require!(
        future.status == FutureStatus::Settled || future.status == FutureStatus::Liquidated,
        FutureError::FutureNotClaimable
    );

    let settlement_amount = future.settlement_amount.ok_or(FutureError::SettlementNotAvailable)?;
    
    require!(
        settlement_amount > 0,
        FutureError::NothingToClaim
    );

    let current_time = contract.get_time()?;

    // Get current prices for conversion (use settlement price if available, otherwise current)
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;

    // Convert settlement amount to tokens
    let claim_tokens = if future.collateral_custody == sol_custody.key() {
        // Claim in SOL
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
        // Claim in USDC
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
    };

    msg!("Claiming {} tokens", claim_tokens);

    // Transfer tokens to user
    if claim_tokens > 0 {
        let claim_token_account = if future.collateral_custody == sol_custody.key() {
            &ctx.accounts.sol_custody_token_account
        } else {
            &ctx.accounts.usdc_custody_token_account
        };

        contract.transfer_tokens(
            claim_token_account.to_account_info(),
            ctx.accounts.receiving_account.to_account_info(),
            ctx.accounts.transfer_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            claim_tokens,
        )?;

        // Update custody balance
        if future.collateral_custody == sol_custody.key() {
            sol_custody.token_owned = math::checked_sub(
                sol_custody.token_owned,
                claim_tokens
            )?;
        } else {
            usdc_custody.token_owned = math::checked_sub(
                usdc_custody.token_owned,
                claim_tokens
            )?;
        }
    }

    // Store values for event before clearing
    let future_key = future.key();
    let owner = future.owner;
    let index = future.index;
    let side = future.side;
    let pnl = future.pnl_at_settlement.unwrap_or(0);
    let settlement_price = future.settlement_price.unwrap_or(0);

    emit!(FutureClaimed {
        owner,
        future_key,
        index,
        side: side as u8,
        settlement_amount,
        claim_tokens,
        pnl,
        settlement_price,
        claim_time: current_time,
    });

    // Clear settlement amount to prevent double claiming
    future.settlement_amount = Some(0);
    future.update_time = current_time;

    // Close account if requested and this is the final claim
    if params.close_account {
        // Return rent to owner
        let future_rent = ctx.accounts.future.to_account_info().lamports();
        **ctx.accounts.future.to_account_info().try_borrow_mut_lamports()? = 0;
        **ctx.accounts.owner.to_account_info().try_borrow_mut_lamports()? = ctx.accounts.owner
            .to_account_info()
            .lamports()
            .checked_add(future_rent)
            .ok_or(ProgramError::ArithmeticOverflow)?;

        // Clear future account data
        let future_info = ctx.accounts.future.to_account_info();
        let mut future_data = future_info.try_borrow_mut_data()?;
        future_data.fill(0);

        emit!(FutureAccountClosed {
            owner,
            future_key,
            index,
            rent_refunded: future_rent,
        });

        msg!("Future account closed and rent returned");
    }

    msg!("Future claim completed successfully");
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: ClaimFutureParams)]
pub struct ClaimFuture<'info> {
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
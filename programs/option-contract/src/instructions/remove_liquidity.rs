//! RemoveLiquidity instruction handler

use {
    crate::{
        errors::ContractError, math, state::{
            custody::Custody,
            oracle::OraclePrice, Contract, Pool,
        }
    },
    anchor_lang::prelude::*,
    anchor_spl::token::{Mint, Token, TokenAccount},
};

#[derive(Accounts)]
#[instruction(params: RemoveLiquidityParams)]
pub struct RemoveLiquidity<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        constraint = receiving_account.mint == custody.mint,
        has_one = owner
    )]
    pub receiving_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = lp_token_account.mint == lp_token_mint.key(),
        has_one = owner
    )]
    pub lp_token_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: empty PDA, authority for token accounts
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
        seeds = [b"pool",
                 params.pool_name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// CHECK: oracle account for the returned token
    #[account(
        constraint = custody_oracle_account.key() == custody.oracle
    )]
    pub custody_oracle_account: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 custody_mint.key().as_ref()],
        bump = custody.token_account_bump
    )]
    pub custody_token_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"lp_token_mint",
                pool.name.as_bytes()],
        bump = pool.lp_token_bump
    )]
    pub lp_token_mint: Box<Account<'info, Mint>>,

    #[account(mut)]
    pub custody_mint: Box<Account<'info, Mint>>,

    token_program: Program<'info, Token>,
    // remaining accounts:
    //   pool.tokens.len() custody accounts (read-only, unsigned)
    //   pool.tokens.len() custody oracles (read-only, unsigned)
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RemoveLiquidityParams {
    pub lp_amount_in: u64,
    pub min_amount_out: u64,
    pub pool_name: String
}

pub fn remove_liquidity<'info>(
    ctx: Context<'_, '_, 'info, 'info, RemoveLiquidity>,
    params: &RemoveLiquidityParams,
) -> Result<()> {
    // check permissions
    msg!("Check permissions");
    let contract = ctx.accounts.contract.as_mut();
    let custody = ctx.accounts.custody.as_mut();
    // validate inputs
    msg!("Validate inputs");
    if params.lp_amount_in == 0 {
        return Err(ProgramError::InvalidArgument.into());
    }
    let pool = ctx.accounts.pool.as_mut();
    let token_id = pool.get_token_id(&custody.key())?;

    // compute assets under management
    msg!("Compute assets under management");
    let curtime = contract.get_time()?;

    // Refresh pool.aum_usm to adapt to token price change
    pool.aum_usd =
        pool.get_assets_under_management_usd(ctx.remaining_accounts, curtime)?;

    let token_price = OraclePrice::new_from_oracle(
        &ctx.accounts.custody_oracle_account.to_account_info(),
        curtime,
        false,
    )?;

    let pool_amount_usd =
        pool.get_assets_under_management_usd(ctx.remaining_accounts, curtime)?;

    // compute amount of tokens to return
    let remove_amount_usd = math::checked_as_u64(math::checked_div(
        math::checked_mul(pool_amount_usd, params.lp_amount_in as u128)?,
        ctx.accounts.lp_token_mint.supply as u128,
    )?)?;

    let remove_amount = token_price.get_token_amount(remove_amount_usd, custody.decimals)?;

    // calculate fee
    let fee_amount =
        pool.get_remove_liquidity_fee(token_id, remove_amount, custody, &token_price)?;
    msg!("Collected fee: {}", fee_amount);

    let transfer_amount = math::checked_sub(remove_amount, fee_amount)?;
    msg!("Amount out: {}", transfer_amount);

    // check pool constraints
    msg!("Check pool constraints");
    let withdrawal_amount = math::checked_add(transfer_amount, fee_amount)?;
    require!(
        pool.check_token_ratio(token_id, 0, withdrawal_amount, custody, &token_price)?,
        ContractError::TokenRatioOutOfRange
    );

    require!(
        math::checked_sub(custody.token_owned, custody.token_locked)? >= withdrawal_amount,
        ContractError::CustodyAmountLimit
    );

    // transfer tokens
    msg!("Transfer tokens");
    contract.transfer_tokens(
        ctx.accounts.custody_token_account.to_account_info(),
        ctx.accounts.receiving_account.to_account_info(),
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        transfer_amount,
    )?;

    // burn lp tokens
    msg!("Burn LP tokens");
    contract.burn_tokens(
        ctx.accounts.lp_token_mint.to_account_info(),
        ctx.accounts.lp_token_account.to_account_info(),
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        params.lp_amount_in,
    )?;

    // update custody stats
    
    custody.token_owned = math::checked_sub(custody.token_owned, withdrawal_amount)?;

    // update pool stats
    msg!("Update pool stats");
    custody.exit(&crate::ID)?;
    pool.aum_usd =
        pool.get_assets_under_management_usd(ctx.remaining_accounts, curtime)?;

    Ok(())
}

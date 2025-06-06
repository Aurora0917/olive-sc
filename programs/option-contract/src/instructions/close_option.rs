use crate::{
    errors::OptionError,
    math,
    state::{Contract, Custody, OptionDetail, OraclePrice, Pool, User},
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CloseOptionParams {
    pub option_index: u64,
    pub pool_name: String,
}

pub fn close_option(ctx: Context<CloseOption>, params: &CloseOptionParams) -> Result<()> {
    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;
    let contract = &ctx.accounts.contract;
    let user = &ctx.accounts.user;
    let transfer_authority = &ctx.accounts.transfer_authority;

    let locked_custody = &mut ctx.accounts.locked_custody;
    let pay_custody = &mut ctx.accounts.pay_custody;
    let pay_custody_token_account = &ctx.accounts.pay_custody_token_account;
    let funding_account = &ctx.accounts.funding_account;
    let pay_custody_oracle_account = &ctx.accounts.pay_custody_oracle_account;
    let custody_oracle_account = &ctx.accounts.custody_oracle_account;

    require_keys_eq!(pay_custody.key(), option_detail.premium_asset);
    require_gte!(user.option_index, params.option_index);

    // Only if option is valid and not exercised
    if option_detail.valid {
        // Get current time and check that option has not expired
        let current_time: i64 = contract.get_time()? as i64;
        if current_time >= option_detail.expired_date {
        // Convert your custom error into a ProgramError and return it:
            return Err(OptionError::InvalidTimeError.into());
        }

        // Reduce locked custody
        require_gte!(
            locked_custody.token_locked,
            option_detail.amount,
            OptionError::InvalidLockedBalanceError
        );
        locked_custody.token_locked = math::checked_sub(locked_custody.token_locked, option_detail.amount)?;

        // Time decay logic
        let remaining_seconds = option_detail.expired_date.saturating_sub(current_time as i64);
        let remaining_days = remaining_seconds as f64 / 86400.0;
        let remaining_years = remaining_days / 365.0;

        // Oracle price of underlying asset (e.g. SOL)
        let underlying_price = OraclePrice::new_from_oracle(
            custody_oracle_account,
            current_time,
            false,
        )?.get_price();

        // Recalculate premium using Black-Scholes
        let bs_price = OptionDetail::black_scholes(
            underlying_price,
            option_detail.strike_price,
            remaining_years,
            option_detail.option_type == 0, // 0 = call, 1 = put
        );

        // Convert premium to token amount
        let pay_token_price = OraclePrice::new_from_oracle(
            pay_custody_oracle_account,
            current_time,
            false,
        )?.get_price();

        let token_decimals = pay_custody.decimals;
        let pay_amount = math::checked_as_u64(
            math::checked_float_div(bs_price, pay_token_price)?
                * math::checked_powi(10.0, token_decimals as i32)?
        )?;

        require_gt!(pay_amount, 0, OptionError::InvalidPayAmountError);

        // Apply 10% fee
        let refund_amount = math::checked_div(math::checked_mul(pay_amount, 9)?, 10)?;

        // Check pool balance
        require_gte!(
            math::checked_sub(pay_custody.token_owned, pay_custody.token_locked)?,
            refund_amount,
            OptionError::InvalidPoolBalanceError
        );

        // Update pool balance
        pay_custody.token_owned = math::checked_sub(pay_custody.token_owned, refund_amount)?;

        // Transfer refund to user
        contract.transfer_tokens(
            pay_custody_token_account.to_account_info(),
            funding_account.to_account_info(),
            transfer_authority.to_account_info(),
            token_program.to_account_info(),
            refund_amount,
        )?;

        // Invalidate option
        option_detail.valid = false;
        option_detail.bought_back = current_time as u64;
    }

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: CloseOptionParams)]
pub struct CloseOption<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        constraint = funding_account.mint == pay_custody.mint,
        has_one = owner
    )]
    pub funding_account: Box<Account<'info, TokenAccount>>,

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
        seeds = [b"user", owner.key().as_ref()],
        bump,
    )]
    pub user: Box<Account<'info, User>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody_mint.key().as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>, // underlying price asset

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 pay_custody_mint.key().as_ref()],
        bump = pay_custody.bump
    )]
    pub pay_custody: Box<Account<'info, Custody>>, // premium payment asset

    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 pay_custody.mint.key().as_ref()],
        bump
    )]
    pub pay_custody_token_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 locked_custody.mint.as_ref()],
        bump = locked_custody.bump
    )]
    pub locked_custody: Box<Account<'info, Custody>>, // locked asset

    #[account(
        mut,
        seeds = [b"option", owner.key().as_ref(),
            params.option_index.to_le_bytes().as_ref(),
            pool.key().as_ref(), custody.key().as_ref()],
        bump
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,

    /// CHECK: oracle for underlying asset
    #[account(constraint = custody_oracle_account.key() == custody.oracle)]
    pub custody_oracle_account: AccountInfo<'info>,

    /// CHECK: oracle for payment asset
    #[account(constraint = pay_custody_oracle_account.key() == pay_custody.oracle)]
    pub pay_custody_oracle_account: AccountInfo<'info>,

    #[account(mut)]
    pub custody_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub pay_custody_mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

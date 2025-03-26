use crate::{
    errors::OptionError,
    math,
    state::{Contract, Custody, OptionDetail, Pool, User},
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Token, TokenAccount},
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

    require_keys_eq!(pay_custody.key(), option_detail.premium_asset);
    require_gte!(user.option_index, params.option_index);

    // If that option wasn't exercised
    if option_detail.valid == true {
        // If that option is call option, restore WSOL amount from locked assets in liquidty pool
        require_gte!(
            locked_custody.token_locked,
            option_detail.amount,
            OptionError::InvalidLockedBalanceError
        );

        locked_custody.token_locked =
            math::checked_sub(locked_custody.token_locked, option_detail.amount)?;

        // Return value to users, remove sale fee.
        let amount = math::checked_div(math::checked_mul(option_detail.premium, 9)?, 10)?;
        require_gte!(
            math::checked_div(pay_custody.token_owned, pay_custody.token_locked)?,
            amount,
            OptionError::InvalidPoolBalanceError
        );
        pay_custody.token_owned = math::checked_sub(pay_custody.token_owned, amount)?;

        contract.transfer_tokens(
            pay_custody_token_account.to_account_info(),
            funding_account.to_account_info(),
            transfer_authority.to_account_info(),
            token_program.to_account_info(),
            amount,
        )?;

        // Disable this option to prevent exercise
        option_detail.valid = false;
        option_detail.bought_back = contract.get_time()? as u64;
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
    bump = user.bump,
  )]
    pub user: Box<Account<'info, User>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>, // premium pay asset

    #[account(
      seeds = [b"option", owner.key().as_ref(), 
            params.option_index.to_le_bytes().as_ref(),
            pool.key().as_ref(), custody.key().as_ref(),],
        bump = option_detail.bump
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 pay_custody.mint.as_ref()],
        bump = pay_custody.bump
    )]
    pub pay_custody: Box<Account<'info, Custody>>, // premium pay asset

    #[account(
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

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

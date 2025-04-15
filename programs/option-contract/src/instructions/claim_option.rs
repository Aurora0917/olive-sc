use crate::{math, state::{Contract, Custody, OptionDetail, Pool, User}};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ClaimOptionParams {
    pub option_index: u64,
    pub pool_name: String
}

pub fn claim_option(ctx: Context<ClaimOption>, params: &ClaimOptionParams) -> Result<()> {
    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;
    let contract = &ctx.accounts.contract;
    let user = &mut ctx.accounts.user;
    let transfer_authority = &mut ctx.accounts.transfer_authority;
    let locked_custody = &mut ctx.accounts.locked_custody;
    let locked_oracle = &ctx.accounts.locked_oracle;
    let funding_account = &mut ctx.accounts.funding_account;
    require_gte!(user.option_index, params.option_index);
    require_eq!(option_detail.valid, false);
    require_gt!(option_detail.claimed, 0);

    require_gte!(math::checked_sub(locked_custody.token_owned, locked_custody.token_locked)?, option_detail.claimed);
    locked_custody.token_owned = math::checked_sub(locked_custody.token_owned, option_detail.claimed)?;
    option_detail.profit = option_detail.claimed;
    option_detail.claimed = 0;
    // send profit to user
    contract.transfer_tokens(
        locked_oracle.to_account_info(),
        funding_account.to_account_info(),
        transfer_authority.to_account_info(),
        token_program.to_account_info(),
        option_detail.profit as u64,
    )?;

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: ClaimOptionParams)]
pub struct ClaimOption<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        constraint = funding_account.mint == locked_custody.mint,
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
    pub custody: Box<Account<'info, Custody>>, // locked asset

    #[account(
      seeds = [b"option", owner.key().as_ref(), 
            params.option_index.to_le_bytes().as_ref(),
            pool.key().as_ref(), custody.key().as_ref(),],
        bump
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 locked_custody.mint.as_ref()],
        bump = locked_custody.bump
    )]
    pub locked_custody: Box<Account<'info, Custody>>, // locked asset

    /// CHECK: oracle account for the position token
    #[account(
        constraint = locked_oracle.key() == locked_custody.oracle
    )]
    pub locked_oracle: AccountInfo<'info>,
    #[account(mut)]
    pub custody_mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

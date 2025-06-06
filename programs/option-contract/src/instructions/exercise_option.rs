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
pub struct ExerciseOptionParams {
    pub option_index: u64,
    pub pool_name: String
}

pub fn exercise_option(ctx: Context<ExerciseOption>, params: &ExerciseOptionParams) -> Result<()> {
    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;
    let contract = &ctx.accounts.contract;
    let user = &mut ctx.accounts.user;
    let funding_account = &mut ctx.accounts.funding_account;
    let transfer_authority = &mut ctx.accounts.transfer_authority;
    let custody: &mut Box<Account<'_, Custody>> = &mut ctx.accounts.custody;
    let locked_custody = &mut ctx.accounts.locked_custody;
    let locked_oracle = &ctx.accounts.locked_oracle;

    require_gte!(user.option_index, params.option_index);
    // Current Unix timestamp
    let current_timestamp = contract.get_time()?;

    // Check if option is available to exercise, before expired time.
    require_gt!(
        option_detail.expired_date,
        current_timestamp as i64,
        OptionError::InvalidTimeError
    );

    let token_price =
        OraclePrice::new_from_oracle(locked_oracle, current_timestamp, false)?;
    let oracle_price = token_price.get_price();

    require_gte!(
        locked_custody.token_locked,
        option_detail.amount,
        OptionError::InvalidLockedBalanceError
    );

    if custody.key() == locked_custody.key() {
        // call option
        require_gte!(
            oracle_price,
            option_detail.strike_price,
            OptionError::InvalidPriceRequirementError
        );
        // Calculate Sol Amount from Option Detail Value : call / covered sol
        let amount = (oracle_price - option_detail.strike_price) * (option_detail.quantity as f64) / oracle_price;

        // send profit to user
        contract.transfer_tokens(
            locked_custody.to_account_info(),
            funding_account.to_account_info(),
            transfer_authority.to_account_info(),
            token_program.to_account_info(),
            amount as u64,
        )?;

        option_detail.profit = amount as u64;
    } else {
        require_gte!(
            option_detail.strike_price,
            oracle_price,
            OptionError::InvalidPriceRequirementError
        );

        // Calculate Profit amount with option detail values:  put / case-secured usdc
        let amount = (option_detail.strike_price - oracle_price) * (option_detail.quantity as f64);

        // send profit to user
        contract.transfer_tokens(
            locked_custody.to_account_info(),
            funding_account.to_account_info(),
            transfer_authority.to_account_info(),
            token_program.to_account_info(),
            amount as u64,
        )?;

        option_detail.profit = amount as u64;
    }

    option_detail.exercised = current_timestamp as u64;
    option_detail.valid = false;

    locked_custody.token_locked =
    math::checked_sub(locked_custody.token_locked, option_detail.amount)?;

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: ExerciseOptionParams)]
pub struct ExerciseOption<'info> {
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
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody_mint.key().as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>, // Target price asset

    #[account(
    seeds = [b"user", owner.key().as_ref()],
    bump,
  )]
    pub user: Box<Account<'info, User>>,

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
                 locked_custody_mint.key().as_ref()],
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
    #[account(mut)]
    pub locked_custody_mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

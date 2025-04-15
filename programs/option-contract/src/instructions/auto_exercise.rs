use crate::{
    errors::OptionError,
    math,
    state::{Contract, Custody, OptionDetail, OraclePrice, Pool, User},
};
use anchor_lang::prelude::*;
use anchor_spl::{associated_token::AssociatedToken, token::{Mint, Token}};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AutoExerciseOptionParams {
    pub user: Pubkey,
    pub option_index: u64,
    pub pool_name: String,
}

pub fn auto_exercise(
    ctx: Context<AutoExerciseOption>,
    params: &AutoExerciseOptionParams,
) -> Result<()> {
    let option_detail = &mut ctx.accounts.option_detail;
    let contract = &ctx.accounts.contract;
    let user = &mut ctx.accounts.user;
    let custody: &mut Box<Account<'_, Custody>> = &mut ctx.accounts.custody;
    let locked_custody = &mut ctx.accounts.locked_custody;
    let locked_oracle = &ctx.accounts.locked_oracle;

    require_gte!(user.option_index, params.option_index);
    // Current Unix timestamp
    let current_timestamp = contract.get_time()?;

    // Check if option is available to exercise, before expired time.
    require_gt!(
        current_timestamp as u64,
        option_detail.expired_date,
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
        if oracle_price > option_detail.strike_price {
            // Calculate Sol Amount from Option Detail Value : call / covered sol
            let amount = ((oracle_price - option_detail.strike_price) / option_detail.strike_price)
                * (option_detail.amount as f64);

            option_detail.claimed = amount as u64;
        } else {
            option_detail.claimed = 0;
            option_detail.profit = 0;
        }
    } else {
        if option_detail.strike_price > oracle_price {
            // Calculate Profit amount with option detail values:  put / case-secured usdc
            let amount =
                (option_detail.strike_price - oracle_price) * (option_detail.amount as f64);

            option_detail.claimed = amount as u64;
        } else {
            option_detail.claimed = 0;
            option_detail.profit = 0;
        }
    }

    option_detail.exercised = current_timestamp as u64;
    option_detail.valid = false;

    locked_custody.token_locked =
        math::checked_sub(locked_custody.token_locked, option_detail.amount)?;

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: AutoExerciseOptionParams)]
pub struct AutoExerciseOption<'info> {
    #[account(mut)]
    pub tester: Signer<'info>,

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
    seeds = [b"user", params.user.key().as_ref()],
    bump,
  )]
    pub user: Box<Account<'info, User>>,

    #[account(
      seeds = [b"option", params.user.key().as_ref(), 
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

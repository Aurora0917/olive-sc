use crate::{
    errors::OptionError,
    state::{Lp, OptionDetail},
};
use anchor_lang::prelude::*;
use anchor_spl::{associated_token::AssociatedToken, token::Token};

pub fn expire_option(ctx: Context<ExpireOption>, option_index: u64) -> Result<()> {
    let option_detail = &mut ctx.accounts.option_detail;
    let lp = &mut ctx.accounts.lp;
    let current_timestamp = Clock::get().unwrap().unix_timestamp;

    require_gt!(
        current_timestamp as u64,
        option_detail.expired_date,
        OptionError::InvalidTimeError
    );
    require_eq!(
        option_index,
        option_detail.index,
        OptionError::InvalidOptionIndexError
    );

    option_detail.valid = false;

    if option_detail.option_type {
        require_gte!(
            lp.locked_sol_amount,
            option_detail.sol_amount,
            OptionError::InvalidLockedBalanceError
        );
        lp.locked_sol_amount -= option_detail.sol_amount;
        lp.sol_amount += option_detail.sol_amount;
    } else {
        require_gte!(
            lp.locked_usdc_amount,
            option_detail.usdc_amount,
            OptionError::InvalidLockedBalanceError
        );
        lp.locked_usdc_amount -= option_detail.usdc_amount;
        lp.usdc_amount += option_detail.usdc_amount;
    }
    Ok(())
}

#[derive(Accounts)]
#[instruction(option_index: u64)]
pub struct ExpireOption<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
      seeds = [b"option", signer.key().as_ref(), &option_index.to_le_bytes()[..]],
      bump,
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,
    #[account(
        seeds = [b"lp"],
        bump,
      )]
    pub lp: Account<'info, Lp>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

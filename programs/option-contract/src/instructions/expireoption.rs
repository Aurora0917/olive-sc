use crate::state::OptionDetail;
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Token,
};

pub fn expire_option(ctx: Context<ExpireOption>, option_index: u64) -> Result<()> {
    let option_detail = &mut ctx.accounts.option_detail;
    let current_timestamp = Clock::get().unwrap().unix_timestamp;

    // TODO: option_detail.expired_date < current_timestamp

    option_detail.valid = false;

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

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

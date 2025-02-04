use anchor_lang::prelude::*;
use crate::utils::*;
use crate::state::{lp::*, Users};

pub fn withdraw(ctx: Context<Withdraw>, amount: u64, iswsol: bool) -> Result<()> {
  let lp = &ctx.accounts.lp;
  let locked_lp = &ctx.accounts.locked_lp;
  let users = &ctx.accounts.users;

  lp.sol_amount = 0;
  lp.usdc_amount = 0;

  locked_lp.sol_amount = 0;
  locked_lp.usdc_amount = 0;

  users.user_count= 0;
  users.max_count = 10;

  
  Ok(())
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
  #[account(mut)]
  pub signer: Signer<'info>,

  #[account(
    init, 
    payer = signer,  
    space=Lp::LEN,
    seeds = [b"lp"],
    bump,
  )]
  pub lp: Account<'info, Lp>,

  #[account(
    init, 
    payer = signer,  
    space=LockedLP::LEN,
    seeds = [b"lockedlp"],
    bump,
  )]
  pub locked_lp: Account<'info, LockedLP>,

  #[account(
    init, 
    payer = signer,  
    space=Users::LEN,
    seeds = [b"users"],
    bump,
  )]
  pub users: Account<'info, Users>,

  system_program: Program<'info, System>,

}
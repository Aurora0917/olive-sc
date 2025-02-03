use anchor_lang::prelude::*;
use crate::utils::*;
use crate::state::lp::*;

pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
  Ok(())
}

#[derive(Accounts)]
pub struct Initialize<'info> {
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

  pub system_program: Program<'info, System>,

}
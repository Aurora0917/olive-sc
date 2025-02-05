use anchor_lang::prelude::*;
use crate::state::{lp::*, LockedLP, Users};
use anchor_spl::{
  associated_token::AssociatedToken,
  token::{Token, Mint, TokenAccount}
};

pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
  let lp = &mut ctx.accounts.lp;
  let locked_lp = &mut ctx.accounts.locked_lp;
  let users = &mut ctx.accounts.users;
  let signer = &ctx.accounts.signer;

  lp.sol_amount = 0;
  lp.usdc_amount = 0;

  locked_lp.sol_amount = 0;
  locked_lp.usdc_amount = 0;

  users.user_count= 0;
  users.max_count = 10;
  users.admin = signer.key();


  Ok(())
}

#[derive(Accounts)]
pub struct Initialize<'info> {
  #[account(mut)]
  pub signer: Signer<'info>,

  pub wsol_mint: Account<'info, Mint>,
  pub usdc_mint: Account<'info, Mint>,

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

  #[account(
    init,
    payer = signer,
    associated_token::mint = wsol_mint,
    associated_token::authority = lp,
  )]
  pub wsol_ata: Account<'info, TokenAccount>,

  #[account(
    init,
    payer = signer,
    associated_token::mint = usdc_mint,
    associated_token::authority = lp,
  )]
  pub usdc_ata: Account<'info, TokenAccount>,

  pub token_program: Program<'info, Token>,
  pub associated_token_program: Program<'info, AssociatedToken>,
  pub system_program: Program<'info, System>,
  pub rent: Sysvar<'info, Rent>,
}
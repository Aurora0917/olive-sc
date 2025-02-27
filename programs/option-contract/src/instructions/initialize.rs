use anchor_lang::prelude::*;
use crate::state::lp::*;
use anchor_spl::{
  associated_token::AssociatedToken,
  token::{Token, Mint, TokenAccount}
};

pub fn initialize(ctx: Context<Initialize>,  bump: u8) -> Result<()> {
  let lp = &mut ctx.accounts.lp;

  lp.sol_amount = 0;
  lp.usdc_amount = 0;
  lp.locked_sol_amount = 0;
  lp.locked_usdc_amount = 0;
  lp.bump = bump;
  Ok(())
}

#[derive(Accounts)]
pub struct Initialize<'info> {
  #[account(mut)]
  pub signer: Signer<'info>,

  pub wsol_mint: Box<Account<'info, Mint>>,
  pub usdc_mint: Box<Account<'info, Mint>>,

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
    associated_token::mint = wsol_mint,
    associated_token::authority = lp,
  )]
  pub wsol_ata: Box<Account<'info, TokenAccount>>,

  #[account(
    init,
    payer = signer,
    associated_token::mint = usdc_mint,
    associated_token::authority = lp,
  )]
  pub usdc_ata: Box<Account<'info, TokenAccount>>,

  pub token_program: Program<'info, Token>,
  pub associated_token_program: Program<'info, AssociatedToken>,
  pub system_program: Program<'info, System>,
}
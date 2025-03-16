use anchor_lang::prelude::*;
use crate::state::{lp::*, Multisig};
use anchor_spl::{
  associated_token::AssociatedToken,
  token::{Token, Mint, TokenAccount}
};

// Create Lp PDA Account and init, store bump.
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

  // Wsol Mint Address
  pub wsol_mint: Box<Account<'info, Mint>>,

  // USDC Mint Address
  pub usdc_mint: Box<Account<'info, Mint>>,

  // Multisig account
  #[account(
    init,
    payer = signer,
    space = Multisig::LEN,
    seeds = [b"multisig"],
    bump
  )]
  pub multisig: AccountLoader<'info, Multisig>,

  // LP PDA account stored Lp status including wsol, usdc account and locked amounts
  #[account(
    init, 
    payer = signer,  
    space=Lp::LEN,
    seeds = [b"lp"],
    bump,
  )]
  pub lp: Account<'info, Lp>,

  // Wsol ATA account of Lp PDA.
  #[account(
    init,
    payer = signer,
    associated_token::mint = wsol_mint,
    associated_token::authority = lp,
  )]
  pub wsol_ata: Box<Account<'info, TokenAccount>>,

  // USDC ATA account of Lp PDA.
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
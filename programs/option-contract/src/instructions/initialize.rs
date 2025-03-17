use anchor_lang::prelude::*;
use crate::state::{Contract, Multisig};
use anchor_spl::token::Token;

// Create Lp PDA Account and init, store bump.
pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
  let contract = &mut ctx.accounts.contract;

  // initialize multisig, this will fail if account is already initialized
  let mut multisig = ctx.accounts.multisig.load_init()?;
  multisig.set_signers(ctx.remaining_accounts, 1)?;

  // store PDA bumps
  contract.bump = ctx.bumps.contract;
  multisig.bump = ctx.bumps.multisig;
  Ok(())
}

#[derive(Accounts)]
pub struct Initialize<'info> {
  #[account(mut)]
  pub signer: Signer<'info>,

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
    space=Contract::LEN,
    seeds = [b"contract"],
    bump,
  )]
  pub contract: Account<'info, Contract>,
  pub token_program: Program<'info, Token>,
  pub system_program: Program<'info, System>,
}
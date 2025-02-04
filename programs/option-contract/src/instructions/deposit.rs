use anchor_lang::prelude::*;
use crate::state::{lp::*};


pub fn deposit(ctx: Context<Deposit>, amount: u64, iswsol: bool) -> Result<()> {
  let signer = &ctx.accounts.signer;
  let lp = &ctx.accounts.lp;
  let users = &ctx.accounts.users;

  lp.sol_amount = 0;
  lp.usdc_amount = 0;

  locked_lp.sol_amount = 0;
  locked_lp.usdc_amount = 0;

  users.user_count= 0;
  users.max_count = 10;

  token::transfer(
    CpiContext::new(
        token_program.to_account_info(),
        SplTransfer {
          from: source.to_account_info(),
          to: lp.to_account_info(),
          authority: signer.to_account_info(),
        },
    ),
    deposit_amount,
  )?;
  
  Ok(())
}

#[derive(Accounts)]
pub struct Deposit<'info> {
  #[account(mut)]
  pub signer: Signer<'info>,

  #[account(
    mut,
    associated_token::mint = wsol_mint,
    associated_token::authority = signer,
  )]
  pub signer_ata: Account<'info, TokenAccount>,

  pub wsol_mint: Account<'info, Mint>,

  #[account(
    init,
    payer = signer,  
    space = Lp::LEN,
    seeds = [b"lp"],
    bump,
  )]
  pub lp: Account<'info, Lp>,

  #[account(
    init_if_needed,
    mut,
    payer = signer,
    associated_token::mint = wsol_mint,
    associated_token::authority = lp,
  )]
  pub lp_ata: Account<'info, TokenAccount>,

  token_program: Program<'info, Token>,
  associated_token_program: Program<'info, AssociatedToken>,
  system_program: Program<'info, System>,

}
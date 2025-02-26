use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer},
};

use crate::{
    errors::PoolError,
    state::Lp,
};

pub fn withdraw_wsol(ctx: Context<WithdrawWsol>, amount: u64, lp_bump: u8) -> Result<()> {
    let signer_ata = &mut ctx.accounts.signer_ata;
    let lp_ata = &mut ctx.accounts.lp_ata;
    let lp = &mut ctx.accounts.lp;

    let token_program = &ctx.accounts.token_program;

    require_gte!(lp_ata.amount, amount, PoolError::InvalidPoolBalanceError);

    lp.sol_amount -= amount;
    token::transfer(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            SplTransfer {
                from: lp_ata.to_account_info(),
                to: signer_ata.to_account_info(),
                authority: lp.to_account_info(),
            },
            &[&[b"lp", &[lp_bump]]],
        ),
        amount,
    )?;

    Ok(())
}

#[derive(Accounts)]
#[instruction(lp_bump: u8)]
pub struct WithdrawWsol<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub wsol_mint: Account<'info, Mint>,

    #[account(
    mut,
    associated_token::mint = wsol_mint,
    associated_token::authority = signer,
  )]
    pub signer_ata: Account<'info, TokenAccount>,

    #[account(
    mut,
    seeds = [b"lp"],
    bump = lp_bump
  )]
    pub lp: Account<'info, Lp>,

    #[account(
      mut,
    associated_token::mint = wsol_mint,
    associated_token::authority = lp,
  )]
    pub lp_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

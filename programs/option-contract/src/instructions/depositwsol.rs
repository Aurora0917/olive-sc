use crate::{errors::PoolError, state::Lp};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer},
};

pub fn deposit_wsol(ctx: Context<DepositWsol>, amount: u64) -> Result<()> {
    let signer = &ctx.accounts.signer;
    let signer_ata = &mut ctx.accounts.signer_ata;
    let lp_ata = &mut ctx.accounts.lp_ata;
    let lp = &mut ctx.accounts.lp;
    let token_program = &ctx.accounts.token_program;

    require_gte!(
        signer_ata.amount,
        amount,
        PoolError::InvalidSignerBalanceError
    );

    token::transfer(
        CpiContext::new(
            token_program.to_account_info(),
            SplTransfer {
                from: signer_ata.to_account_info(),
                to: lp_ata.to_account_info(),
                authority: signer.to_account_info(),
            },
        ),
        amount,
    )?;
    lp.sol_amount += amount;
    Ok(())
}

#[derive(Accounts)]
pub struct DepositWsol<'info> {
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
    seeds = [b"lp"],
    bump,
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

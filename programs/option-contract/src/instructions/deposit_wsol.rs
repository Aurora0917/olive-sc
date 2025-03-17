use crate::{
    errors::PoolError,
    state::{Lp, User},
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer},
};

pub fn deposit_wsol(ctx: Context<DepositWsol>, amount: u64) -> Result<()> {
    let signer = &ctx.accounts.signer;
    let user = &mut ctx.accounts.user;
    let signer_ata = &mut ctx.accounts.signer_ata;
    let lp_ata = &mut ctx.accounts.lp_ata;
    let lp = &mut ctx.accounts.lp;
    let token_program = &ctx.accounts.token_program;

    // Check if user balance is enough to deposit amount to liquidity pool
    require_gte!(
        signer_ata.amount,
        amount,
        PoolError::InvalidSignerBalanceError
    );

    // Transfer WSOL from users to liquidity pool
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

    // Add liquidty pool amount
    lp.sol_amount += amount;

    // Add deposited amount by user
    user.liquidity_wsol += amount;
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
    mut,
    seeds = [b"lp"],
    bump,
  )]
    pub lp: Box<Account<'info, Lp>>,

    #[account(
    mut,
    associated_token::mint = wsol_mint,
    associated_token::authority = lp,
  )]
    pub lp_ata: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        space=User::LEN,
        seeds = [b"user", signer.key().as_ref()],
        bump,
      )]
    pub user: Box<Account<'info, User>>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

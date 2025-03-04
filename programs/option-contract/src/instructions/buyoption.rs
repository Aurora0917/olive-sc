use std::ops::{Div, Mul};

use crate::{
    errors::OptionError,
    state::{Lp, OptionDetail, User},
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer},
};

pub fn buy_option(ctx: Context<BuyOption>, option_index: u64) -> Result<()> {
    let signer_ata_wsol = &mut ctx.accounts.signer_ata_wsol;
    let lp_ata_wsol = &mut ctx.accounts.lp_ata_wsol;
    let signer_ata_usdc = &mut ctx.accounts.signer_ata_usdc;
    let lp_ata_usdc = &mut ctx.accounts.lp_ata_usdc;
    let lp = &mut ctx.accounts.lp;
    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;
    let current_timestamp = Clock::get().unwrap().unix_timestamp;

    require_eq!(
        option_index,
        option_detail.index,
        OptionError::InvalidOptionIndexError
    );

    if option_detail.valid == true {
        let amount = option_detail.premium.mul(9).div(10);

        if option_detail.option_type {
            require_gte!(
                lp.locked_sol_amount,
                option_detail.sol_amount,
                OptionError::InvalidLockedBalanceError
            );
            lp.locked_sol_amount -= option_detail.sol_amount;
            lp.sol_amount += option_detail.sol_amount;
        } else {
            require_gte!(
                lp.locked_usdc_amount,
                option_detail.usdc_amount,
                OptionError::InvalidLockedBalanceError
            );
            lp.locked_usdc_amount -= option_detail.usdc_amount;
            lp.usdc_amount += option_detail.usdc_amount;
        }
        if option_detail.option_type {
            require_gte!(
                lp_ata_wsol.amount,
                amount,
                OptionError::InvalidPoolBalanceError
            );
            lp.sol_amount -= amount;
            token::transfer(
                CpiContext::new_with_signer(
                    token_program.to_account_info(),
                    SplTransfer {
                        from: lp_ata_wsol.to_account_info(),
                        to: signer_ata_wsol.to_account_info(),
                        authority: lp.to_account_info(),
                    },
                    &[&[&b"lp"[..], &[lp.bump]]],
                ),
                amount,
            )?;
        } else {
            require_gte!(
                lp_ata_usdc.amount,
                amount,
                OptionError::InvalidPoolBalanceError
            );
            lp.usdc_amount -= amount;
            token::transfer(
                CpiContext::new_with_signer(
                    token_program.to_account_info(),
                    SplTransfer {
                        from: lp_ata_usdc.to_account_info(),
                        to: signer_ata_usdc.to_account_info(),
                        authority: lp.to_account_info(),
                    },
                    &[&[&b"lp"[..], &[lp.bump]]],
                ),
                amount,
            )?;
        }
        option_detail.valid = false;
        option_detail.bought_back = current_timestamp as u64;
    }

    Ok(())
}

#[derive(Accounts)]
#[instruction(option_index: u64)]
pub struct BuyOption<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub wsol_mint: Account<'info, Mint>,
    pub usdc_mint: Account<'info, Mint>,

    #[account(
    mut,
    associated_token::mint = wsol_mint,
    associated_token::authority = signer,
    )]
    pub signer_ata_wsol: Account<'info, TokenAccount>,

    #[account(
    mut,
    associated_token::mint = usdc_mint,
    associated_token::authority = signer,
    )]
    pub signer_ata_usdc: Account<'info, TokenAccount>,

    #[account(
      mut,
  seeds = [b"lp"],
  bump = lp.bump,
)]
    pub lp: Account<'info, Lp>,

    #[account(
        mut,
  associated_token::mint = wsol_mint,
  associated_token::authority = lp,
)]
    pub lp_ata_wsol: Account<'info, TokenAccount>,

    #[account(
        mut,
    associated_token::mint = usdc_mint,
    associated_token::authority = lp,
  )]
    pub lp_ata_usdc: Account<'info, TokenAccount>,

    #[account(
        mut,
  seeds = [b"user", signer.key().as_ref()],
  bump,
)]
    pub user: Box<Account<'info, User>>,

    #[account(mut,
        seeds = [b"option", signer.key().as_ref(), option_index.to_le_bytes().as_ref()],
        bump)]
    pub option_detail: Box<Account<'info, OptionDetail>>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

use crate::{
    errors::OptionError,
    state::{Lp, OptionDetail},
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer},
};
pub fn expire_option(ctx: Context<ExpireOption>, option_index: u64, price: f64) -> Result<()> {
    let option_detail = &mut ctx.accounts.option_detail;
    let lp = &mut ctx.accounts.lp;
    let current_timestamp = Clock::get().unwrap().unix_timestamp;
    let token_program = &ctx.accounts.token_program;
    let signer_ata_wsol = &mut ctx.accounts.signer_ata_wsol;
    let signer_ata_usdc = &mut ctx.accounts.signer_ata_usdc;
    let lp_ata_usdc = &mut ctx.accounts.lp_ata_usdc;
    let lp_ata_wsol = &mut ctx.accounts.lp_ata_wsol;

    require_gt!(
        current_timestamp as u64,
        option_detail.expired_date,
        OptionError::InvalidTimeError
    );
    require_eq!(
        option_index,
        option_detail.index,
        OptionError::InvalidOptionIndexError
    );
    let amount: f64;
    if price > option_detail.strike_price && option_detail.option_type {
        require_gte!(
            lp.locked_sol_amount,
            option_detail.sol_amount,
            OptionError::InvalidLockedBalanceError
        );
        lp.locked_sol_amount -= option_detail.sol_amount;
        lp.sol_amount += option_detail.sol_amount;

        // call / covered sol
        amount = ((price - option_detail.strike_price) / option_detail.strike_price)
            * (option_detail.sol_amount as f64);

        // send profit to user
        token::transfer(
            CpiContext::new_with_signer(
                token_program.to_account_info(),
                SplTransfer {
                    from: lp_ata_wsol.to_account_info(),
                    to: signer_ata_wsol.to_account_info(),
                    authority: lp.to_account_info(),
                },
                &[&[b"lp", &[lp.bump]]],
            ),
            amount as u64,
        )?;

        option_detail.exercised = current_timestamp as u64;
        option_detail.valid = false;
        option_detail.profit = amount as u64;
        option_detail.profit_unit = true;
    } else if price < option_detail.strike_price && !option_detail.option_type {
        require_gte!(
            lp.locked_usdc_amount,
            option_detail.usdc_amount,
            OptionError::InvalidLockedBalanceError
        );
        lp.locked_usdc_amount -= option_detail.usdc_amount;
        lp.usdc_amount += option_detail.usdc_amount;

        // put / case-secured usdc
        amount = (option_detail.strike_price - price) / price * (option_detail.usdc_amount as f64);

        // send profit to user
        token::transfer(
            CpiContext::new_with_signer(
                token_program.to_account_info(),
                SplTransfer {
                    from: lp_ata_usdc.to_account_info(),
                    to: signer_ata_usdc.to_account_info(),
                    authority: lp.to_account_info(),
                },
                &[&[b"lp", &[lp.bump]]],
            ),
            amount as u64,
        )?;

        option_detail.exercised = current_timestamp as u64;
        option_detail.valid = false;
        option_detail.profit = amount as u64;
        option_detail.profit_unit = false;
    } else {
        option_detail.valid = false;
        option_detail.profit = 0;
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
    }
    Ok(())
}

#[derive(Accounts)]
#[instruction(option_index: u64)]
pub struct ExpireOption<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(mut)]
    pub option_detail: Box<Account<'info, OptionDetail>>,

    pub wsol_mint: Account<'info, Mint>,
    pub usdc_mint: Account<'info, Mint>,

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
    pub lp_ata_wsol: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
          associated_token::mint = usdc_mint,
          associated_token::authority = lp,
        )]
    pub lp_ata_usdc: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = wsol_mint,
        associated_token::authority = signer,
      )]
    pub signer_ata_wsol: Box<Account<'info, TokenAccount>>,

    #[account(
          mut,
          associated_token::mint = usdc_mint,
          associated_token::authority = signer,
        )]
    pub signer_ata_usdc: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

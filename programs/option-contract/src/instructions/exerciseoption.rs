use crate::{
    errors::OptionError,
    state::{Lp, OptionDetail, User},
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer},
};
use pyth_sdk_solana::{state::SolanaPriceAccount, PriceFeed};

pub fn exercise_option(ctx: Context<ExerciseOption>, option_index: u64) -> Result<()> {
    let signer_ata_wsol = &mut ctx.accounts.signer_ata_wsol;
    let signer_ata_usdc = &mut ctx.accounts.signer_ata_usdc;
    let lp_ata_usdc = &mut ctx.accounts.lp_ata_usdc;
    let lp_ata_wsol = &mut ctx.accounts.lp_ata_wsol;
    let lp = &mut ctx.accounts.lp;
    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;
    let price_account_info = &ctx.accounts.pyth_price_account;

    let current_timestamp = Clock::get().unwrap().unix_timestamp;

    require_eq!(
        option_index,
        option_detail.index,
        OptionError::InvalidOptionIndexError
    );
    require_gt!(
        option_detail.expired_date,
        current_timestamp as u64,
        OptionError::InvalidTimeError
    );

    let current_timestamp = Clock::get().unwrap().unix_timestamp;
    let price_feed: PriceFeed =
        SolanaPriceAccount::account_info_to_feed(price_account_info).unwrap();
    let price = price_feed
        .get_price_no_older_than(current_timestamp, 60)
        .unwrap(); // Ensure price is not older than 60 seconds
    let oracle_price = (price.price as f64) * 10f64.powi(price.expo);

    let amount: f64;
    if option_detail.option_type {
        require_gte!(
            oracle_price,
            option_detail.strike_price,
            OptionError::InvalidPriceRequirementError
        );
        require_gte!(
            lp.locked_sol_amount,
            option_detail.sol_amount,
            OptionError::InvalidLockedBalanceError
        );
        lp.locked_sol_amount -= option_detail.sol_amount;
        lp.sol_amount += option_detail.sol_amount;

        // call / covered sol
        amount = ((oracle_price - option_detail.strike_price) / option_detail.strike_price)
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
    } else {
        require_gte!(
            option_detail.strike_price,
            oracle_price,
            OptionError::InvalidPriceRequirementError
        );

        require_gte!(
            lp.locked_usdc_amount,
            option_detail.usdc_amount,
            OptionError::InvalidLockedBalanceError
        );
        lp.locked_usdc_amount -= option_detail.usdc_amount;
        lp.usdc_amount += option_detail.usdc_amount;

        // put / case-secured usdc
        amount = (option_detail.strike_price - oracle_price) / oracle_price
            * (option_detail.usdc_amount as f64);

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
    }
    Ok(())
}

#[derive(Accounts)]
#[instruction(option_index: u64)]
pub struct ExerciseOption<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub wsol_mint: Account<'info, Mint>,
    pub usdc_mint: Account<'info, Mint>,

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

    #[account(
        mut,
    seeds = [b"lp"],
    bump,
  )]
    pub lp: Account<'info, Lp>,

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
    seeds = [b"user", signer.key().as_ref()],
    bump,
  )]
    pub user: Box<Account<'info, User>>,

    #[account(mut)]
    pub option_detail: Box<Account<'info, OptionDetail>>,
    /// CHECK:
    pub pyth_price_account: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

use crate::{state::{Lp, OptionDetail, User}, utils::SOL_PRICE_ID};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer},
};
use pyth_solana_receiver_sdk::price_update::{get_feed_id_from_hex, PriceUpdateV2};

pub fn exercise_option(ctx: Context<ExerciseOption>, option_index: u64) -> Result<()> {
    let signer = &ctx.accounts.signer;
    let signer_ata_wsol = &mut ctx.accounts.signer_ata_wsol;
    let signer_ata_usdc = &mut ctx.accounts.signer_ata_usdc;
    let lp_ata_usdc = &mut ctx.accounts.lp_ata_usdc;
    let lp_ata_wsol = &mut ctx.accounts.lp_ata_wsol;
    let price_update = &mut ctx.accounts.price_update;

    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;

    let current_timestamp = Clock::get().unwrap().unix_timestamp;

    require_eq!(option_index, option_detail.index);
    require_gt!(option_detail.expired_date, current_timestamp as u64);

    // TODO: get price from oracle
    let feed_id: [u8; 32] = get_feed_id_from_hex(SOL_PRICE_ID)?;
    let price = price_update.get_price_no_older_than(&Clock::get()?, 30, &feed_id)?;


    let oracle_price = (price.price as f64) * 10f64.powi(price.exponent);

    let amount: f64;
    if option_detail.sol_amount > 0 {
        // call / covered sol
        amount = ((oracle_price - option_detail.strike_price) / option_detail.strike_price) * (option_detail.sol_amount as f64);

        // send profit to user
        token::transfer(
            CpiContext::new(
                token_program.to_account_info(),
                SplTransfer {
                    from: lp_ata_wsol.to_account_info(),
                    to: signer_ata_wsol.to_account_info(),
                    authority: signer.to_account_info(),
                },
            ),
            amount as u64,
        )?;

        option_detail.exercised = current_timestamp as u64;
        option_detail.valid = false;
        option_detail.profit = amount as u64;
        option_detail.profit_unit = true;
    } else if option_detail.usdc_amount > 0 {
        // put / case-secured usdc
        amount = (option_detail.strike_price - oracle_price) / oracle_price * (option_detail.usdc_amount as f64);

        // send profit to user
        token::transfer(
            CpiContext::new(
                token_program.to_account_info(),
                SplTransfer {
                    from: lp_ata_usdc.to_account_info(),
                    to: signer_ata_usdc.to_account_info(),
                    authority: signer.to_account_info(),
                },
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
    pub signer_ata_wsol: Account<'info, TokenAccount>,

    #[account(
      mut,
      associated_token::mint = usdc_mint,
      associated_token::authority = signer,
    )]
    pub signer_ata_usdc: Account<'info, TokenAccount>,

    #[account(
    seeds = [b"lp"],
    bump,
  )]
    pub lp: Account<'info, Lp>,

    #[account(
    associated_token::mint = wsol_mint,
    associated_token::authority = lp,
  )]
    pub lp_ata_wsol: Account<'info, TokenAccount>,

    #[account(
      associated_token::mint = usdc_mint,
      associated_token::authority = lp,
    )]
    pub lp_ata_usdc: Account<'info, TokenAccount>,

    #[account(
    seeds = [b"user", signer.key().as_ref()],
    bump,
  )]
    pub user: Box<Account<'info, User>>,

    #[account(
      seeds = [b"option", signer.key().as_ref(), &option_index.to_le_bytes()[..]],
      bump,
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,
    pub price_update: Account<'info, PriceUpdateV2>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

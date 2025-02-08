use crate::{state::{Lp, OptionDetail, User}, utils::SOL_PRICE_ID};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer},
};
use pyth_solana_receiver_sdk::price_update::{get_feed_id_from_hex, PriceUpdateV2};

pub fn sell_option(
    ctx: Context<SellOption>,
    amount: u64,
    strike: f64,
    period: f64,
    option_index: u64,
) -> Result<()> {
    let signer = &ctx.accounts.signer;
    let signer_ata = &mut ctx.accounts.signer_ata;
    let lp_ata = &mut ctx.accounts.lp_ata;
    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;
    let user = &mut ctx.accounts.user;
    
    require_eq!(option_index, user.option_index+1);

    let price_update = &mut ctx.accounts.price_update;
    let feed_id: [u8; 32] = get_feed_id_from_hex(SOL_PRICE_ID)?;
    let price = price_update.get_price_no_older_than(&Clock::get()?, 30, &feed_id)?;

    let oracle_price = (price.price as f64) * 10f64.powi(price.exponent);

    //calc premium
    let period_sqrt = period.sqrt(); // Using floating-point sqrt
    let iv = 0.6;
    let ratio = strike / oracle_price;
    let premium = period_sqrt
        * iv
        * if ratio > 1.0 {
            // call - covered sol option
            oracle_price / strike
        } else {
            // put - cash secured usdc option
            strike / oracle_price
        };

    // TODO : Pay sol or usdc if this is call or put
    require_gte!(signer_ata.amount, premium as u64);
    // send premium to pool
    token::transfer(
        CpiContext::new(
            token_program.to_account_info(),
            SplTransfer {
                from: signer_ata.to_account_info(),
                to: lp_ata.to_account_info(),
                authority: signer.to_account_info(),
            },
        ),
        premium as u64,
    )?;

    // store option data
    option_detail.index = option_index;
    option_detail.sol_amount = amount;
    option_detail.expired_date = period as u64;
    option_detail.strike_price = strike;
    option_detail.premium = premium as u64;
    option_detail.premium_unit = ratio > 1.0;
    option_detail.valid = true;

    // store user/users data
    user.option_index = option_index;

    //TODO: locked assets after option

    Ok(())
}

#[derive(Accounts)]
#[instruction(option_index: u64)]
pub struct SellOption<'info> {
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

    #[account(
      init,
      payer = signer,
      space=OptionDetail::LEN,
      seeds = [b"option", signer.key().as_ref(), &option_index.to_le_bytes()[..]],
      bump,
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,
    pub price_update: Account<'info, PriceUpdateV2>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

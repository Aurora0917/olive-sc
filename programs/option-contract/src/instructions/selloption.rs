use std::ops::Div;

use crate::{
    errors::OptionError,
    state::{Lp, OptionDetail, User}, utils::{USDC_DECIMALS, WSOL_DECIMALS},
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer},
};
use pyth_sdk_solana::state::SolanaPriceAccount;

pub fn sell_option(
    ctx: Context<SellOption>,
    amount: u64,
    strike: f64,
    period: u64,       // number day
    expired_time: u64, // when the option is expired
    option_index: u64,
    is_call: bool, // true : call option, false : put option
    pay_sol: bool, // true : sol, false : usdc
) -> Result<()> {
    let signer = &ctx.accounts.signer;
    let signer_ata_wsol = &mut ctx.accounts.signer_ata_wsol;
    let signer_ata_usdc = &mut ctx.accounts.signer_ata_usdc;
    let lp_ata_wsol = &mut ctx.accounts.lp_ata_wsol;
    let lp_ata_usdc = &mut ctx.accounts.lp_ata_usdc;
    let lp = &mut ctx.accounts.lp;
    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;
    let user = &mut ctx.accounts.user;

    require_eq!(
        option_index,
        user.option_index + 1,
        OptionError::InvalidOptionIndexError
    );
    let price_account_info = &ctx.accounts.pyth_price_account;
    let price_feed = SolanaPriceAccount::account_info_to_feed(price_account_info)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    let price = price_feed.get_price_unchecked();
    msg!("price_feed: {:?}", price);
    let oracle_price = (price.price as f64) * 10f64.powi(price.expo);
    msg!("oracle_price: {}", oracle_price);
    //calc premium
    let period_sqrt = (period as f64).sqrt(); // Using floating-point sqrt
    let iv = 0.6;
    let premium = period_sqrt
        * iv
        * if is_call {
            // call - covered sol option
            oracle_price / strike
        } else {
            // put - cash secured usdc option
            strike / oracle_price
        };
    let premium_sol = (premium.div(oracle_price) * i32::pow(10, WSOL_DECIMALS) as f64) as u64;
    let premium_usdc = (premium * i32::pow(10, USDC_DECIMALS) as f64) as u64;
    msg!("premium_sol: {}", premium_sol);

    if pay_sol {
        require_gte!(
            signer_ata_wsol.amount,
            premium_sol,
            OptionError::InvalidSignerBalanceError
        );
        token::transfer(
            CpiContext::new(
                token_program.to_account_info(),
                SplTransfer {
                    from: signer_ata_wsol.to_account_info(),
                    to: lp_ata_wsol.to_account_info(),
                    authority: signer.to_account_info(),
                },
            ),
            premium_sol,
        )?;
    } else {
        require_gte!(
            signer_ata_usdc.amount,
            premium_usdc,
            OptionError::InvalidSignerBalanceError
        );
        // send premium to pool
        msg!("signer_ata_usdc: {}", signer_ata_usdc.key());
        msg!("lp_ata_usdc: {}", lp_ata_usdc.key());

        token::transfer(
            CpiContext::new(
                token_program.to_account_info(),
                SplTransfer {
                    from: signer_ata_usdc.to_account_info(),
                    to: lp_ata_usdc.to_account_info(),
                    authority: signer.to_account_info(),
                },
            ),
            premium_usdc,
        )?;
    }
    // Lock assets for call(covered sol)/ put(secured-cash usdc) option
    if is_call {
        require_gte!(lp.sol_amount, amount, OptionError::InvalidPoolBalanceError);
        lp.sol_amount += premium as u64;
        lp.locked_sol_amount += premium as u64;
        lp.sol_amount -= premium as u64;
    } else {
        require_gte!(lp.usdc_amount, amount, OptionError::InvalidPoolBalanceError);
        lp.usdc_amount += premium as u64;
        lp.locked_usdc_amount += premium as u64;
        lp.usdc_amount -= premium as u64;
    }

    // store option data
    option_detail.index = option_index;
    option_detail.sol_amount = amount;
    option_detail.period = period;
    option_detail.expired_date = expired_time as u64;
    option_detail.strike_price = strike;
    option_detail.premium = premium as u64;
    option_detail.premium_unit = pay_sol;
    option_detail.option_type = is_call;
    option_detail.valid = true;
    user.option_index = option_index;

    Ok(())
}

#[derive(Accounts)]
pub struct SellOption<'info> {
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
    bump=lp.bump,
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
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,
    /// CHECK:
    pub pyth_price_account: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

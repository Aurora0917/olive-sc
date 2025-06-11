use crate::{
    errors::OptionError,
    math,
    state::{Contract, Custody, OptionDetail, OraclePrice, Pool, User},
};
use anchor_lang::prelude::*;
use anchor_spl::
    token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct OpenOptionParams {
    amount: u64, // WSOL/USDC account for options, call option - SOL amount, Put option - USDC amount
    strike: f64, // Strike price
    period: u64, // Number of days from option creation to expiration
    expired_time: u64, // when the option is expired : Unix epoch time
    pool_name : String,
}

pub fn open_option(ctx: Context<OpenOption>, params: &OpenOptionParams) -> Result<()> {
    let owner = &ctx.accounts.owner;
    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;
    let contract = &ctx.accounts.contract;
    let user = &mut ctx.accounts.user;
    let pool = &ctx.accounts.pool;
    let custody = &mut ctx.accounts.custody;
    let custody_oracle_account = &ctx.accounts.custody_oracle_account;
    let locked_custody = &mut ctx.accounts.locked_custody;

    let pay_custody = &mut ctx.accounts.pay_custody;
    let pay_custody_oracle_account = &ctx.accounts.pay_custody_oracle_account;
    let pay_custody_token_account = &ctx.accounts.pay_custody_token_account;

    let funding_account = &ctx.accounts.funding_account;

    let option_index = user.option_index + 1;
    // compute position price
    let curtime = contract.get_time()?;

    // Check if the user's token balance is enough to pay premium
    require_gte!(
        funding_account.amount,
        params.amount,
        OptionError::InvalidSignerBalanceError
    );

    // Send Pay token from User to Pool Custody as premium
    token::transfer(
        CpiContext::new(
            token_program.to_account_info(),
            SplTransfer {
                from: funding_account.to_account_info(),
                to: pay_custody_token_account.to_account_info(),
                authority: owner.to_account_info(),
            },
        ),
        params.amount,
    )?;
    
    let token_price = OraclePrice::new_from_oracle(custody_oracle_account, curtime, false)?;

    let oracle_price = token_price.get_price();
    let period_year = math::checked_as_f64(math::checked_float_div(params.period as f64, 365.0)?)?;

    msg!("oracle_price: {}", oracle_price);
    msg!("params.strike: {}", params.strike);
    msg!("period_year: {}", period_year);
    // Calculate Premium in usd using black scholes formula.
    let premium = OptionDetail::black_scholes(
        oracle_price,
        params.strike,
        period_year,
        custody.key() == locked_custody.key(),
    );
    msg!("premium: {}", premium);

    let pay_token_price = OraclePrice::new_from_oracle(pay_custody_oracle_account, curtime, false)?;

    // Calculate Premium in pay_toke amount
    let pay_amount = math::checked_as_u64(
        math::checked_float_div(premium, pay_token_price.get_price())?
            * math::checked_powi(10.0, pay_custody.decimals as i32)?,
    )?;

    require_gt!(
        pay_amount,
        0,
        OptionError::InvalidPayAmountError
    );

    // Add premium to liquidity pool
    pay_custody.token_owned = math::checked_add(pay_custody.token_owned, params.amount)?;
    option_detail.premium = pay_amount;
    option_detail.premium_asset = pay_custody.key();

    let quantity = math::checked_div(params.amount, pay_amount)?;
    msg!("quantity: {}", quantity);

    let decimals_multiplier = math::checked_powi(10.0, pay_custody.decimals as i32)?;
    locked_custody.token_locked = math::checked_add(
        locked_custody.token_locked,
        math::checked_as_u64(quantity as f64 * decimals_multiplier)?
    )?;

    require_gte!(
        locked_custody.token_owned,
        locked_custody.token_locked,
        OptionError::InvalidPoolBalanceError
    );

    // store option data
    option_detail.amount = params.amount;
    option_detail.quantity = quantity;
    option_detail.owner = owner.key();
    option_detail.index = option_index;
    option_detail.period = params.period;
    option_detail.expired_date = params.expired_time as i64;
    option_detail.purchase_date = curtime as u64;
    option_detail.option_type = if custody.key() == locked_custody.key() { 0 } else { 1 };
    option_detail.strike_price = params.strike;
    option_detail.valid = true;
    option_detail.locked_asset = locked_custody.key();
    option_detail.pool = pool.key();
    option_detail.custody = custody.key();
    user.option_index = option_index;

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: OpenOptionParams)]
pub struct OpenOption<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(mut)]
    pub funding_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: empty PDA, authority for token accounts
    #[account(
        seeds = [b"transfer_authority"],
        bump = contract.transfer_authority_bump
    )]
    pub transfer_authority: AccountInfo<'info>,

    #[account(
        seeds = [b"contract"],
        bump = contract.bump
    )]
    pub contract: Box<Account<'info, Contract>>,

    #[account(
        mut,
        seeds = [b"pool",
                 params.pool_name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody_mint.key().as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>, // Target price asset

    /// CHECK: oracle account for the position token
    #[account(
        constraint = custody_oracle_account.key() == custody.oracle
    )]
    pub custody_oracle_account: AccountInfo<'info>,

    #[account(
    init_if_needed,
    payer = owner,
    space=User::LEN,
    seeds = [b"user", owner.key().as_ref()],
    bump,
  )]
    pub user: Box<Account<'info, User>>,

    #[account(
      init,
      payer = owner,
      space=OptionDetail::LEN,
      seeds = [b"option", owner.key().as_ref(), 
            (user.option_index+1).to_le_bytes().as_ref(),
            pool.key().as_ref(), custody.key().as_ref()],
        bump
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 pay_custody_mint.key().as_ref()],
        bump = pay_custody.bump
    )]
    pub pay_custody: Box<Account<'info, Custody>>, // premium pay asset

    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 pay_custody.mint.key().as_ref()],
        bump
    )]
    pub pay_custody_token_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: oracle account for the position token
    #[account(
        constraint = pay_custody_oracle_account.key() == pay_custody.oracle
    )]
    pub pay_custody_oracle_account: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 locked_custody_mint.key().as_ref()],
        bump = locked_custody.bump
    )]
    pub locked_custody: Box<Account<'info, Custody>>, // locked asset
    #[account(mut)]
    pub custody_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub pay_custody_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub locked_custody_mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

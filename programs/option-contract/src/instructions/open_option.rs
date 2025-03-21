use crate::{
    errors::OptionError,
    math,
    state::{Contract, Custody, OptionDetail, OraclePrice, Pool, User},
    utils::black_scholes,
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Token, TokenAccount, Transfer as SplTransfer},
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct OpenOptionParams {
    amount: u64, // WSOL/USDC account for options, call option - SOL amount, Put option - USDC amount
    strike: f64, // Strike price
    period: u64, // Number of days from option creation to expiration
    expired_time: u64, // when the option is expired : Unix epoch time
}

pub fn open_option(ctx: Context<SellOption>, params: OpenOptionParams) -> Result<()> {
    let owner = &ctx.accounts.owner;
    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;
    let contract = &ctx.accounts.contract;
    let pool = &ctx.accounts.pool;
    let user = &mut ctx.accounts.user;

    let custody = &ctx.accounts.custody;
    let custody_oracle_account = &ctx.accounts.custody_oracle_account;
    let locked_custody = &ctx.accounts.locked_custody;

    let pay_custody = &ctx.accounts.pay_custody;
    let pay_custody_oracle_account = &ctx.accounts.pay_custody_oracle_account;
    let pay_custody_token_account = &ctx.accounts.pay_custody_token_account;

    let funding_account = &ctx.accounts.funding_account;

    let option_index = user.option_index + 1;
    // compute position price
    let curtime = contract.get_time()?;

    let token_price = OraclePrice::new_from_oracle(custody_oracle_account, curtime, false)?;

    let oracle_price = token_price.get_price();
    let period_year = math::checked_as_f64(math::checked_float_div(params.period as f64, 365.0)?)?;

    // Calculate Premium in usd using black scholes formula.
    let premium = black_scholes(oracle_price, params.strike, period_year, params.is_call);

    let pay_custody_id = pool.get_token_id(&ctx.accounts.custody.key())?;
    let pay_token_price = OraclePrice::new_from_oracle(pay_custody_oracle_account, curtime, false)?;

    // Calculate Premium in pay_toke amount
    let pay_amount = math::checked_as_u64(
        math::checked_float_div(premium, pay_token_price.get_price())?
            * math::checked_powi(10.0, pay_custody.decimals as i32)?,
    )?;

    // Check if the user's token balance is enough to pay premium
    require_gte!(
        funding_account.amount,
        pay_amount,
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
        pay_amount,
    )?;

    // Add premium to liquidity pool
    pay_custody.assets.owned = math::checked_add(pay_custody.assets.owned, pay_amount)?;
    option_detail.premium = premium_sol;

    // Lock assets for call(covered sol)/ put(secured-cash usdc) option
    if custody.key() == locked_custody.key() { // Call
        require_gte!(custody.assets.owned , math::checked_sub(custody.assets.locked, params.amount)? , OptionError::InvalidPoolBalanceError);
        custody.assets.locked += params.amount as u64;

        
        lp.sol_amount -= amount as u64;
        option_detail.sol_amount = amount;
    } else {
        require_gte!(lp.usdc_amount, amount, OptionError::InvalidPoolBalanceError);
        lp.locked_usdc_amount += amount as u64;
        lp.usdc_amount -= amount as u64;
        option_detail.usdc_amount = amount;
    }

    // store option data
    option_detail.index = option_index;
    option_detail.period = period;
    option_detail.expired_date = expired_time as u64;
    option_detail.strike_price = strike;
    option_detail.premium_unit = pay_sol;
    option_detail.option_type = is_call;
    option_detail.valid = true;
    user.option_index = option_index;

    Ok(())
}

#[derive(Accounts)]
pub struct SellOption<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        constraint = funding_account.mint == pay_custody.mint,
        has_one = owner
    )]
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
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>,

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
            pool.key().as_ref(), custody.key().as_ref(),],
        bump
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 pay_custody.mint.as_ref()],
        bump = pay_custody.bump
    )]
    pub pay_custody: Box<Account<'info, Custody>>,

    #[account(
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 pay_custody.mint.key().as_ref()],
        bump
    )]
    pub pay_custody_token_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: oracle account for the position token
    #[account(
        constraint = custody_oracle_account.key() == pay_custody.oracle
    )]
    pub pay_custody_oracle_account: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 locked_custody.mint.as_ref()],
        bump = locked_custody.bump
    )]
    pub locked_custody: Box<Account<'info, Custody>>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

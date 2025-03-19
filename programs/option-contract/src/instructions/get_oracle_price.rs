//! GetOraclePrice instruction handler

use {
    crate::state::{custody::Custody, oracle::OraclePrice, pool::Pool, Contract},
    anchor_lang::prelude::*,
};

#[derive(Accounts)]
pub struct GetOraclePrice<'info> {
    #[account(
        seeds = [b"contract"],
        bump = contract.bump
    )]
    pub contract: Box<Account<'info, Contract>>,

    #[account(
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// CHECK: oracle account for the collateral token
    #[account(
        constraint = custody_oracle_account.key() == custody.oracle
    )]
    pub custody_oracle_account: AccountInfo<'info>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetOraclePriceParams {
    ema: bool,
}

pub fn get_oracle_price(
    ctx: Context<GetOraclePrice>,
    params: &GetOraclePriceParams,
) -> Result<u64> {
    let curtime = ctx.accounts.contract.get_time()?;

    let price = OraclePrice::new_from_oracle(
        &ctx.accounts.custody_oracle_account.to_account_info(),
        curtime,
        params.ema,
    )?;

    Ok(price
        .scale_to_exponent(-(Contract::PRICE_DECIMALS as i32))?
        .price)
}

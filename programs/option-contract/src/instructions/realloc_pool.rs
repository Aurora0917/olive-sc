use anchor_lang::prelude::*;
use anchor_spl::token::Token;

use crate::state::{Contract, Multisig, Pool, TokenRatios};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ReallocPoolParams {
    pub ratios: Vec<TokenRatios>,
    pub custody_key : Pubkey,
    pub pool_name : String,
}

pub fn realloc_pool(
    ctx: Context<RealocPool>,
    params: &ReallocPoolParams,
) -> Result<()> {
    // validate inputs
    if params.ratios.len() != ctx.accounts.pool.ratios.len() + 1 {
        return Err(ProgramError::InvalidArgument.into());
    }

    let pool =&mut ctx.accounts.pool;
    if pool.get_token_id(&params.custody_key).is_ok() {
        // return error if custody is already initialized
        return Err(ProgramError::AccountAlreadyInitialized.into());
    }

    // update pool data
    pool.custodies.push(params.custody_key);
    pool.ratios = params.ratios.clone();
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: ReallocPoolParams)]
pub struct RealocPool<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        mut,
        seeds = [b"multisig"],
        bump = multisig.load()?.bump
    )]
    pub multisig: AccountLoader<'info, Multisig>,

    #[account(
        mut,
        seeds = [b"contract"],
        bump = contract.bump,
      )]
    pub contract: Box<Account<'info, Contract>>,

    #[account(
        mut,
        realloc = Pool::LEN + (pool.custodies.len() + 1) * std::mem::size_of::<Pubkey>() +
        (pool.ratios.len() + 1) * std::mem::size_of::<TokenRatios>(),
        realloc::payer = signer,
        realloc::zero = false,
        seeds = [b"pool", params.pool_name.as_bytes()],
        bump = pool.bump,
    )]
    pub pool: Account<'info, Pool>,

    token_program: Program<'info, Token>,
    system_program: Program<'info, System>,
}

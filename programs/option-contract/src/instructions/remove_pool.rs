use anchor_lang::prelude::*;

use crate::{errors::PoolError, state::{multisig::{AdminInstruction, Multisig}, Contract, Pool}};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RemovePoolParams {
    pool_name: String
}

pub fn remove_pool<'info>(ctx: Context<'_, '_, '_, 'info, RemovePool<'info>>, params: &RemovePoolParams) -> Result<u8> {
    // validate signatures
    let mut multisig = ctx.accounts.multisig.load_mut()?;
    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.signer,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::AddPool, params)?,
    )?;

    if signatures_left > 0 {
        msg!(
            "Instruction has been signed but more signatures are required: {}",
            signatures_left
        );
        return Ok(signatures_left);
    }

    require!(
        ctx.accounts.pool.custodies.is_empty(),
        PoolError::InvalidPoolState
    );

    // remove pool from the list
    let contract = &mut ctx.accounts.contract.as_mut();
    let pool_idx = contract
        .pools
        .iter()
        .position(|x| *x == ctx.accounts.pool.key())
        .ok_or(PoolError::InvalidPoolState)?;
    contract.pools.remove(pool_idx);

    Ok(0)
}


#[derive(Accounts)]
#[instruction(params: RemovePoolParams)]
pub struct RemovePool<'info> {
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
        realloc= Contract::LEN +(contract.pools.len() - 1) * std::mem::size_of::<Pubkey>(),
        realloc::payer = signer,
        realloc::zero = false,
        seeds = [b"contract"],
        bump = contract.bump,
      )]
      pub contract: Box<Account<'info, Contract>>,
    
    #[account(
        mut,
        seeds = [b"pool", params.pool_name.as_bytes()],
        bump,
        close = signer
    )]
    pub pool: Box<Account<'info, Pool>>,

    system_program: Program<'info, System>,
}
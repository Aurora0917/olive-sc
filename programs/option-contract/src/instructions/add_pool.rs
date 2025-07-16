use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};

use crate::{
    events::PoolAdded,
    state::{Contract, multisig::{Multisig, AdminInstruction}, Pool}
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AddPoolParams {
    pub name: String,
}

pub fn add_pool<'info>(ctx: Context<'_, '_, '_, 'info, AddPool<'info>>, params: &AddPoolParams) -> Result<u8> {
    // validate inputs
    if params.name.is_empty() || params.name.len() > 64 {
        return Err(ProgramError::InvalidArgument.into());
    }

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

    // record contract data
    let contract =&mut ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    if pool.bump !=0 {
        return Err(ProgramError::AccountAlreadyInitialized.into());
    }

    pool.name = params.name.clone();
    pool.bump = ctx.bumps.pool;
    pool.lp_token_bump = ctx.bumps.lp_token_mint;
    
    // Initialize borrow rate curve with default parameters
    pool.initialize_borrow_rate_curve()?;
    
    // Initialize rate tracking fields
    pool.cumulative_funding_rate_long = 0;
    pool.cumulative_funding_rate_short = 0;
    pool.cumulative_interest_rate = 0;
    pool.last_rate_update = Clock::get()?.unix_timestamp;
    
    // Initialize open interest tracking
    pool.long_open_interest_usd = 0;
    pool.short_open_interest_usd = 0;
    pool.total_borrowed_usd = 0;
    pool.last_utilization_update = Clock::get()?.unix_timestamp;
    
    contract.pools.push(pool.key());
    
    emit!(PoolAdded {
        pool: pool.key(),
        name: pool.name.clone(),
        lp_token_mint: ctx.accounts.lp_token_mint.key(),
        bump: pool.bump,
        lp_token_bump: pool.lp_token_bump,
        cumulative_funding_rate_long: pool.cumulative_funding_rate_long,
        cumulative_funding_rate_short: pool.cumulative_funding_rate_short,
        cumulative_interest_rate: pool.cumulative_interest_rate,
        long_open_interest_usd: pool.long_open_interest_usd,
        short_open_interest_usd: pool.short_open_interest_usd,
        total_borrowed_usd: pool.total_borrowed_usd,
    });

    Ok(0)
}


#[derive(Accounts)]
#[instruction(params: AddPoolParams)]
pub struct AddPool<'info> {
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
        realloc= Contract::LEN +(contract.pools.len() + 1) * std::mem::size_of::<Pubkey>(),
        realloc::payer = signer,
        realloc::zero = false,
        seeds = [b"contract"],
        bump = contract.bump,
      )]
      pub contract: Box<Account<'info, Contract>>,
    
    #[account(
        init_if_needed,
        payer = signer,
        space = Pool::LEN,
        seeds = [b"pool", params.name.as_bytes()],
        bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        init_if_needed,
        payer = signer,
        mint::authority = transfer_authority,
        mint::freeze_authority = transfer_authority,
        mint::decimals = Contract::LP_DECIMALS,
        seeds = [b"lp_token_mint",
            params.name.as_bytes()],
        bump
    )]
    pub lp_token_mint: Box<Account<'info, Mint>>,

    /// CHECK: empty PDA, authority for token accounts
    #[account(
        seeds = [b"transfer_authority"],
        bump = contract.transfer_authority_bump
    )]
    pub transfer_authority: AccountInfo<'info>,

    token_program: Program<'info, Token>,
    system_program: Program<'info, System>,
    rent: Sysvar<'info, Rent>
}
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};

use crate::state::{Contract, multisig::{Multisig, AdminInstruction}, Pool};

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
    contract.pools.push(ctx.accounts.pool.key());
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
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::{
    errors::PoolError,
    state::{
        multisig::{AdminInstruction, Multisig}, Contract, Custody, Pool, TokenRatios
    },
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RemoveCustodyParams {
    pub ratios: Vec<TokenRatios>,
    pub pool_name: String
}

pub fn remove_custody<'info>(
    ctx: Context<'_, '_, '_, 'info, RemoveCustody<'info>>,
    params: &RemoveCustodyParams,
) -> Result<u8> {
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
        ctx.accounts.custody_token_account.amount == 0,
        PoolError::InvalidCustodyState
    );
    // remove token from the list
    let pool = ctx.accounts.pool.as_mut();
    let token_id = pool.get_token_id(&ctx.accounts.custody.key())?;
    pool.custodies.remove(token_id);
    pool.ratios = params.ratios.clone();

    Contract::close_token_account(
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.custody_token_account.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.transfer_authority.to_account_info(),
        &[&[
            b"transfer_authority",
            &[ctx.accounts.contract.transfer_authority_bump],
        ]],
    )?;

    Ok(0)
}

#[derive(Accounts)]
#[instruction(params: RemoveCustodyParams)]
pub struct RemoveCustody<'info> {
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
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody_token_mint.key().as_ref()],
        bump
    )]
    pub custody: Account<'info, Custody>,

    #[account(
        token::mint = custody_token_mint,
        token::authority = transfer_authority, // PDA
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 custody_token_mint.key().as_ref()],
        bump = custody.token_account_bump
    )]
    pub custody_token_account: Account<'info, TokenAccount>,

    /// CHECK: empty PDA, authority for token accounts
    #[account(
        seeds = [b"transfer_authority"],
        bump = contract.transfer_authority_bump
    )]
    pub transfer_authority: AccountInfo<'info>,

    pub custody_token_mint: Box<Account<'info, Mint>>,
    token_program: Program<'info, Token>,
    system_program: Program<'info, System>,
}

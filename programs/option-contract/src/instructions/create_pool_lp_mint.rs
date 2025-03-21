use anchor_lang::prelude::*;
use anchor_spl::{token::{Mint, Token}, token_interface::{token_metadata_initialize, TokenMetadataInitialize}};

use crate::state::Contract;

#[derive(AnchorDeserialize, AnchorSerialize, Clone)]
pub struct LpTokenMintData {
    pub name: String, // Token name == Pool name
    pub symbol: String, // Token Symbol
    pub uri: String, // Token URI
}

pub fn create_pool_lp_mint<'info>(ctx: Context<'_, '_, '_, 'info, CreatLpMint<'info>>, params: &LpTokenMintData) -> Result<()> {
    let cpi_accounts = TokenMetadataInitialize {
        token_program_id: ctx.accounts.token_program.to_account_info(),
        mint: ctx.accounts.lp_token_mint.to_account_info(),
        metadata: ctx.accounts.lp_token_mint.to_account_info(), // metadata account is the mint, since data is stored in mint
        mint_authority: ctx.accounts.transfer_authority.to_account_info(),
        update_authority: ctx.accounts.transfer_authority.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
    token_metadata_initialize(cpi_ctx, params.name.clone(), params.symbol.clone(), params.uri.clone())?;
    Ok(())
}


#[derive(Accounts)]
#[instruction(params: LpTokenMintData)]
pub struct CreatLpMint<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        mut, 
        seeds = [b"contract"],
        bump = contract.bump,
      )]
      pub contract: Box<Account<'info, Contract>>,
    
    #[account(
        init,
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
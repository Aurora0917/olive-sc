use anchor_lang::prelude::*;
use anchor_spl::{token::{Mint, Token}, token_interface::{token_metadata_initialize, TokenMetadataInitialize}};

use crate::state::Contract;

#[derive(AnchorDeserialize, AnchorSerialize, Clone)]
pub struct LpTokenMintData {
    pub name: String, // Token name == Pool name
    pub symbol: String, // Token Symbol
    pub uri: String, // Token URI
}

pub fn create_lp_mint(_ctx: Context<CreatLpMint>, _params: &LpTokenMintData) -> Result<()> {
     
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
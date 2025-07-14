//! AddLiquidity instruction handler with ONLY metadata addition

use {
    crate::{
        errors::ContractError, math, state::{
            custody::Custody, oracle::OraclePrice, Contract, Pool
        }
    },
    anchor_lang::prelude::*,
    anchor_spl::{
        associated_token::AssociatedToken, 
        token::{Mint, Token, TokenAccount},
        metadata::{
            create_metadata_accounts_v3,
            CreateMetadataAccountsV3,
        }
    },
};

// Token Metadata Program ID constant
const TOKEN_METADATA_PROGRAM_ID: Pubkey = anchor_lang::solana_program::pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

#[derive(Accounts)]
#[instruction(params: AddLiquidityParams)]
pub struct AddLiquidity<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(mut)]
    pub funding_account: Box<Account<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer= owner,
        associated_token::mint = lp_token_mint,
        associated_token::authority = owner,
    )]
    pub lp_token_account: Box<Account<'info, TokenAccount>>,

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
    pub custody: Box<Account<'info, Custody>>,

    /// CHECK: oracle account for the receiving token
    #[account(
        constraint = custody_oracle_account.key() == custody.oracle
    )]
    pub custody_oracle_account: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.token_account_bump
    )]
    pub custody_token_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"lp_token_mint",
                pool.name.as_bytes()],
        bump = pool.lp_token_bump
    )]
    pub lp_token_mint: Box<Account<'info, Mint>>,

    #[account(mut)]
    pub custody_mint: Box<Account<'info, Mint>>,

    // === METADATA ACCOUNTS (ONLY NEW ADDITION) ===
    /// CHECK: Metadata account for LP token
    #[account(
        mut,
        seeds = [
            b"metadata",
            TOKEN_METADATA_PROGRAM_ID.as_ref(),
            lp_token_mint.key().as_ref()
        ],
        bump,
        seeds::program = TOKEN_METADATA_PROGRAM_ID
    )]
    pub lp_token_metadata: UncheckedAccount<'info>,

    /// CHECK: Token Metadata Program
    #[account(address = TOKEN_METADATA_PROGRAM_ID)]
    pub token_metadata_program: UncheckedAccount<'info>,
    // === END METADATA ACCOUNTS ===

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,

    // remaining accounts:
    //   pool.tokens.len() custody accounts (read-only, unsigned)
    //   pool.tokens.len() custody oracles (read-only, unsigned)
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct AddLiquidityParams {
    amount_in: u64,
    min_lp_amount_out: u64,
    pool_name: String,
}

pub fn add_liquidity<'info>(ctx: Context<'_, '_, 'info, 'info, AddLiquidity<'info>>, params: &AddLiquidityParams) -> Result<()> {
    // check permissions
    if params.amount_in == 0 {
        return Err(ProgramError::InvalidArgument.into());
    }

    // === METADATA CREATION (ONLY NEW ADDITION) ===
    let metadata_exists = !ctx.accounts.lp_token_metadata.data_is_empty();
    if !metadata_exists {
        create_lp_token_metadata(&ctx, &params.pool_name)?;
    }
    // === END METADATA CREATION ===

    let contract = ctx.accounts.contract.as_mut();
    let custody = ctx.accounts.custody.as_mut();
    let pool = ctx.accounts.pool.as_mut();
    let token_id = pool.get_token_id(&custody.key())?;

    // calculate fee
    let curtime = contract.get_time()?;
    // Refresh pool.aum_usm to adapt to token price change
    pool.aum_usd =
        pool.get_assets_under_management_usd(ctx.remaining_accounts, curtime)?;

    let token_price = OraclePrice::new_from_oracle(
        &ctx.accounts.custody_oracle_account.to_account_info(),
        curtime,
        false,
    )?;

    let fee_amount =
        pool.get_add_liquidity_fee(token_id, params.amount_in, custody, &token_price)?;
    msg!("Collected fee: {}", fee_amount);

    let deposit_amount = math::checked_sub(params.amount_in, fee_amount)?;

    // transfer tokens
    msg!("Transfer tokens");
    contract.transfer_tokens_from_user(
        ctx.accounts.funding_account.to_account_info(),
        ctx.accounts.custody_token_account.to_account_info(),
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        params.amount_in,
    )?;

    // compute assets under management
    msg!("Compute assets under management");
    let pool_amount_usd =
        pool.get_assets_under_management_usd(ctx.remaining_accounts, curtime)?;

    // compute amount of lp tokens to mint
    let no_fee_amount = math::checked_sub(params.amount_in, fee_amount)?;
    require_gte!(
        no_fee_amount,
        1u64,
        ContractError::InsufficientAmountReturned
    );

    let token_amount_usd = token_price.get_asset_amount_usd(no_fee_amount, custody.decimals)?;

    let lp_amount = if pool_amount_usd == 0 {
        token_amount_usd
    } else {
        math::checked_as_u64(math::checked_div(
            math::checked_mul(
                token_amount_usd as u128,
                ctx.accounts.lp_token_mint.supply as u128,
            )?,
            pool_amount_usd,
        )?)?
    };
    msg!("LP tokens to mint: {}", lp_amount);

    // mint lp tokens
    contract.mint_tokens(
        ctx.accounts.lp_token_mint.to_account_info(),
        ctx.accounts.lp_token_account.to_account_info(),
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        lp_amount,
    )?;
    custody.token_owned = math::checked_add(custody.token_owned, deposit_amount)?;

    // update pool stats
    msg!("Update pool stats");
    custody.exit(&crate::ID)?;
    pool.aum_usd =
        pool.get_assets_under_management_usd(ctx.remaining_accounts, curtime)?;

    Ok(())
}

// === METADATA FUNCTIONS (ONLY NEW ADDITION) ===
fn create_lp_token_metadata<'info>(
    ctx: &Context<'_, '_, 'info, 'info, AddLiquidity<'info>>,
    pool_name: &str,
) -> Result<()> {
    let name = format!("Olive {} LP Token", pool_name);
    let symbol = format!("O-{}", pool_name.replace("-", ""));
    let uri = "https://gateway.pinata.cloud/ipfs/bafkreieb6uffibah2qreznrhpodicg44tbnymhoprhqkg4p35ek4zxn5om".to_string();

    let transfer_authority_bump = ctx.accounts.contract.transfer_authority_bump;
    let seeds = &[b"transfer_authority".as_ref(), &[transfer_authority_bump]];
    let signer = &[&seeds[..]];

    let metadata_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_metadata_program.to_account_info(),
        CreateMetadataAccountsV3 {
            metadata: ctx.accounts.lp_token_metadata.to_account_info(),
            mint: ctx.accounts.lp_token_mint.to_account_info(),
            mint_authority: ctx.accounts.transfer_authority.to_account_info(),
            update_authority: ctx.accounts.transfer_authority.to_account_info(),
            payer: ctx.accounts.owner.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            rent: ctx.accounts.rent.to_account_info(),
        },
        signer,
    );

    // Use the embedded mpl_token_metadata types from anchor_spl
    let data = anchor_spl::metadata::mpl_token_metadata::types::DataV2 {
        name,
        symbol,
        uri,
        seller_fee_basis_points: 0,
        creators: Some(vec![
            anchor_spl::metadata::mpl_token_metadata::types::Creator {
                address: ctx.accounts.transfer_authority.key(),
                verified: true,
                share: 100,
            }
        ]),
        collection: None,
        uses: None,
    };

    create_metadata_accounts_v3(
        metadata_ctx,
        data,
        false, // is_mutable
        true,  // update_authority_is_signer
        None,  // collection_details
    )?;

    Ok(())
}
#![allow(unexpected_cfgs)]
#![allow(clippy::result_large_err)]
use anchor_lang::prelude::*;
use instructions::*;

pub mod errors;
pub mod instructions;
pub mod math;
pub mod state;

declare_id!("GSmqNhxAhrLJjcxd9G2ts3obF9va9QBRezm6PMQJuE9b");

#[program]
pub mod option_contract {
    use super::*;
    // Initialize smart contract Accounts
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        instructions::initialize::initialize(ctx)
    }

    // Add admins as multisig signers
    pub fn set_signers<'info>(
        ctx: Context<'_, '_, '_, 'info, SetAdminSigners<'info>>,
        params: SetAdminSignersParams,
    ) -> Result<u8> {
        instructions::set_signers::set_signers(ctx, &params)
    }

    // Create LP token for each Pool
    pub fn create_lp_mint(ctx: Context<CreatLpMint>, params: LpTokenMintData) -> Result<()> {
        instructions::create_lp_mint::create_lp_mint(ctx, &params)
    }

    // Add Pool with multi sig
    pub fn add_pool<'info>(
        ctx: Context<'_, '_, '_, 'info, AddPool<'info>>,
        params: AddPoolParams,
    ) -> Result<u8> {
        instructions::add_pool::add_pool(ctx, &params)
    }

    // Remove Pool with multi sig
    pub fn remove_pool<'info>(
        ctx: Context<'_, '_, '_, 'info, RemovePool<'info>>,
        params: RemovePoolParams,
    ) -> Result<u8> {
        instructions::remove_pool::remove_pool(ctx, &params)
    }

    // Make Storate in Pool for new custody
    pub fn realloc_pool(ctx: Context<RealocPool>, params: ReallocPoolParams) -> Result<()> {
        instructions::realloc_pool::realloc_pool(ctx, &params)
    }

    // Add Custody with multi sig
    pub fn add_custody<'info>(
        ctx: Context<'_, '_, '_, 'info, AddCustody<'info>>,
        params: AddCustodyParams,
    ) -> Result<u8> {
        instructions::add_custody::add_custody(ctx, &params)
    }
    // Remove Custody with multi sig
    pub fn remove_custody<'info>(
        ctx: Context<'_, '_, '_, 'info, RemoveCustody<'info>>,
        params: RemoveCustodyParams,
    ) -> Result<u8> {
        instructions::remove_custody::remove_custody(ctx, &params)
    }

    // Add liquidity 
    pub fn add_liquidity<'info>(
        ctx: Context<'_, '_, 'info, 'info, AddLiquidity<'info>>,
        params: AddLiquidityParams,
    ) -> Result<()> {
        instructions::add_liquidity::add_liquidity(ctx, &params)
    }
    // Remove liquidity
    pub fn remove_liquidity<'info>(
        ctx: Context<'_, '_, 'info, 'info, RemoveLiquidity<'info>>,
        params: RemoveLiquidityParams,
    ) -> Result<()> {
        instructions::remove_liquidity::remove_liquidity(ctx, &params)
    }

    pub fn open_limit_option(ctx: Context<OpenLimitOption>, params: OpenLimitOptionParams) -> Result<()> {
        instructions::open_limit_option::open_limit_option(ctx, &params)
    }

    // Buy option from user to liquidity pool before expired time by user
    pub fn close_limit_option(ctx: Context<CloseLimitOption>, params: CloseLimitOptionParams) -> Result<()> {
        instructions::close_limit_option::close_limit_option(ctx, &params)
    }

    // Sell option froom liquidity to user
    pub fn open_option(ctx: Context<OpenOption>, params: OpenOptionParams) -> Result<()> {
        instructions::open_option::open_option(ctx, &params)
    }

    // Buy option from user to liquidity pool before expired time by user
    pub fn close_option(ctx: Context<CloseOption>, params: CloseOptionParams) -> Result<()> {
        instructions::close_option::close_option(ctx, &params)
    }

    // Exercise option before expired time by user
    pub fn exercise_option(
        ctx: Context<ExerciseOption>,
        params: ExerciseOptionParams,
    ) -> Result<()> {
        instructions::exercise_option::exercise_option(ctx, &params)
    }

    // Exercise option after expired time by bot
    pub fn auto_exercise(
        ctx: Context<AutoExerciseOption>,
        params: AutoExerciseOptionParams,
    ) -> Result<()> {
        instructions::auto_exercise::auto_exercise(ctx, &params)
    }

    // Claim "in the money" option after expired time by user
    pub fn claim_option(ctx: Context<ClaimOption>, params: ClaimOptionParams) -> Result<()> {
        instructions::claim_option::claim_option(ctx, &params)
    }
}

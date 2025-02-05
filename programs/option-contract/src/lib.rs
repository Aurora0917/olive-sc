use anchor_lang::prelude::*;
use instructions::*;

pub mod errors;
pub mod instructions;
pub mod state;
pub mod utils;

declare_id!("DYTHL9fkyWvVEMUPeUZWqVtDMNv8joYdvTD21UWhKkeN");

#[program]
pub mod option_contract {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        instructions::initialize::initialize(ctx)
    }

    pub fn withdraw_usdc(ctx: Context<WithdrawUsdc>, amount: u64, bump: u8) -> Result<()> {
        instructions::withdrawusdc::withdraw_usdc(ctx, amount, bump)
    }

    pub fn withdraw_wsol(ctx: Context<WithdrawWsol>, amount: u64, bump: u8) -> Result<()> {
        instructions::withdrawwsol::withdraw_wsol(ctx, amount, bump)
    }

    pub fn deposit_wsol(ctx: Context<DepositWsol>, amount: u64) -> Result<()> {
        instructions::depositwsol::deposit_wsol(ctx, amount)
    }

    pub fn deposit_usdc(ctx: Context<DepositUsdc>, amount: u64) -> Result<()> {
        instructions::depositusdc::deposit_usdc(ctx, amount)
    }
}

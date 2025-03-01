#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use instructions::*;

pub mod errors;
pub mod instructions;
pub mod state;
pub mod utils;

declare_id!("6h756PU3oXMfQhUXkvcUjspGf9BpYqRUvYPhgQgc3owQ");

#[program]
pub mod option_contract {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, bump: u8) -> Result<()> {
        instructions::initialize::initialize(ctx, bump)
    }

    pub fn withdraw_usdc(ctx: Context<WithdrawUsdc>, amount: u64) -> Result<()> {
        instructions::withdrawusdc::withdraw_usdc(ctx, amount)
    }

    pub fn withdraw_wsol(ctx: Context<WithdrawWsol>, amount: u64) -> Result<()> {
        instructions::withdrawwsol::withdraw_wsol(ctx, amount)
    }

    pub fn deposit_wsol(ctx: Context<DepositWsol>, amount: u64) -> Result<()> {
        instructions::depositwsol::deposit_wsol(ctx, amount)
    }

    pub fn deposit_usdc(ctx: Context<DepositUsdc>, amount: u64) -> Result<()> {
        instructions::depositusdc::deposit_usdc(ctx, amount)
    }

    pub fn sell_option(
        ctx: Context<SellOption>,
        option_index: u64,
        amount: u64,
        strike: f64,
        period: u64,
        expired_time: u64,
        is_call: bool,
        pay_sol: bool,
    ) -> Result<()> {
        instructions::selloption::sell_option(
            ctx,
            option_index,
            amount,
            strike,
            period,
            expired_time,
            is_call,
            pay_sol,
        )
    }

    pub fn exercise_option(ctx: Context<ExerciseOption>, option_index: u64) -> Result<()> {
        instructions::exerciseoption::exercise_option(ctx, option_index)
    }

    pub fn expire_option(ctx: Context<ExpireOption>, option_index: u64, price: f64) -> Result<()> {
        instructions::expireoption::expire_option(ctx, option_index, price)
    }

    pub fn buy_option(ctx: Context<BuyOption>, option_index: u64) -> Result<()> {
        instructions::buyoption::buy_option(ctx, option_index)
    }
}

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
    // Initialize smart contract Accounts - Lp PDA
    pub fn initialize(ctx: Context<Initialize>, bump: u8) -> Result<()> {
        instructions::initialize::initialize(ctx, bump)
    }

    // Withdraw USDC from Liquidity Pool
    pub fn withdraw_usdc(ctx: Context<WithdrawUsdc>, amount: u64) -> Result<()> {
        instructions::withdrawusdc::withdraw_usdc(ctx, amount)
    }

    // Withdraw WSOL from Liquidity Pool
    pub fn withdraw_wsol(ctx: Context<WithdrawWsol>, amount: u64) -> Result<()> {
        instructions::withdrawwsol::withdraw_wsol(ctx, amount)
    }

    // Deposit WSOL from Liquidity Pool
    pub fn deposit_wsol(ctx: Context<DepositWsol>, amount: u64) -> Result<()> {
        instructions::depositwsol::deposit_wsol(ctx, amount)
    }

    // Deposit USDC from Liquidity Pool
    pub fn deposit_usdc(ctx: Context<DepositUsdc>, amount: u64) -> Result<()> {
        instructions::depositusdc::deposit_usdc(ctx, amount)
    }

    // Sell option froom liquidity to user
    pub fn sell_option(
        ctx: Context<SellOption>,
        amount: u64,
        strike: f64,
        period: u64,
        expired_time: u64,
        is_call: bool,
        pay_sol: bool,
    ) -> Result<()> {
        instructions::selloption::sell_option(
            ctx,
            amount,
            strike,
            period,
            expired_time,
            is_call,
            pay_sol,
        )
    }

    // Exercise option before expired time by user
    pub fn exercise_option(ctx: Context<ExerciseOption>, option_index: u64) -> Result<()> {
        instructions::exerciseoption::exercise_option(ctx, option_index)
    }

    // Exercise option after expired time by user
    pub fn expire_option(ctx: Context<ExpireOption>, option_index: u64, price: f64) -> Result<()> {
        instructions::expireoption::expire_option(ctx, option_index, price)
    }

    // Buy option from user to liquidity pool before expired time by user
    pub fn buy_option(ctx: Context<BuyOption>, option_index: u64) -> Result<()> {
        instructions::buyoption::buy_option(ctx, option_index)
    }
}

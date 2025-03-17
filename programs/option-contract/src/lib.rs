#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use instructions::*;

pub mod errors;
pub mod instructions;
pub mod state;
pub mod utils;
pub mod math;

declare_id!("6h756PU3oXMfQhUXkvcUjspGf9BpYqRUvYPhgQgc3owQ");

#[program]
pub mod option_contract {
    use super::*;
    // Initialize smart contract Accounts - Lp PDA
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        instructions::initialize::initialize(ctx)
    }

    // Withdraw USDC from Liquidity Pool
    pub fn withdraw_usdc(ctx: Context<WithdrawUsdc>, amount: u64) -> Result<()> {
        instructions::withdraw_usdc::withdraw_usdc(ctx, amount)
    }

    // Withdraw WSOL from Liquidity Pool
    pub fn withdraw_wsol(ctx: Context<WithdrawWsol>, amount: u64) -> Result<()> {
        instructions::withdraw_wsol::withdraw_wsol(ctx, amount)
    }

    // Deposit WSOL from Liquidity Pool
    pub fn deposit_wsol(ctx: Context<DepositWsol>, amount: u64) -> Result<()> {
        instructions::deposit_wsol::deposit_wsol(ctx, amount)
    }

    // Deposit USDC from Liquidity Pool
    pub fn deposit_usdc(ctx: Context<DepositUsdc>, amount: u64) -> Result<()> {
        instructions::deposit_usdc::deposit_usdc(ctx, amount)
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
        instructions::sell_option::sell_option(
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
        instructions::exercise_option::exercise_option(ctx, option_index)
    }

    // Exercise option after expired time by user
    pub fn expire_option(ctx: Context<ExpireOption>, option_index: u64, price: f64) -> Result<()> {
        instructions::expire_option::expire_option(ctx, option_index, price)
    }

    // Buy option from user to liquidity pool before expired time by user
    pub fn buy_option(ctx: Context<BuyOption>, option_index: u64) -> Result<()> {
        instructions::buy_option::buy_option(ctx, option_index)
    }
}

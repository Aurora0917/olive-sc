use anchor_lang::prelude::*;

#[account]
pub struct Lp {
    pub sol_amount: u64,
    pub usdc_amount: u64,
    pub locked_sol_amount: u64,
    pub locked_usdc_amount: u64,
}

impl Lp {
    pub const LEN: usize = 8 * 4 + 8;
}
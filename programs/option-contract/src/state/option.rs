use anchor_lang::prelude::*;

#[account]
pub struct OptionDetail {
    pub index: u64,
    pub sol_amount: u64,
    pub usdc_amount: u64,
    pub expired_date: u64,
    pub strike_price: u64,
    pub bought_back: bool,
    pub exercised: u64,
    pub profit: u64,
    pub profit_unit: bool, // sol - 1, usdc - 0
    pub premium: u64,
    pub premium_unit: bool, // sol - 1, usdc - 0
}

impl OptionDetail {
    pub const LEN: usize = 8 + 8 + 8 + 8 + 8 + 1 + 8 + 8 + 1 + 8 + 1 + 8;
}

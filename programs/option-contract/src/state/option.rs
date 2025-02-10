use anchor_lang::prelude::*;

#[account]
pub struct OptionDetail {
    pub index: u64,
    pub sol_amount: u64,
    pub usdc_amount: u64,
    pub expired_date: u64,
    pub strike_price: f64,
    pub bought_back: u64,
    pub exercised: u64,
    pub valid: bool, // false - invalid/expried/exercised, true - valid
    pub profit: u64,
    pub profit_unit: bool, // sol - 1, usdc - 0
    pub premium: u64,
    pub premium_unit: bool, // sol - 1, usdc - 0
    pub option_type: bool,  // call - 1, put - 0
}

impl OptionDetail {
    pub const LEN: usize = 8 * 8 + 1 + 1 + 8 + 1 * 2 + 8;
}

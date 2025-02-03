use anchor_lang::prelude::*;

#[account]
pub struct LockedLP {
 pub sol_amount : u64,
 pub usdc_amount : u64,
}

impl LockedLP {
    pub const LEN: usize = 8*2 + 8;
}
use anchor_lang::prelude::*;

#[account]
pub struct User {
    pub option_index: u64,
    pub bump: u8,
}

impl User {
    pub const LEN: usize = 8 * 1 + 1 + 8;
}

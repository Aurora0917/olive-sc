use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;

#[account]
pub struct Users {
    pub user_number: u64,
    #[max_len(10, 32)]
    pub users: Vec<Pubkey>, // add resize for size
}

impl Users {
    pub const LEN: usize = 8 + 32 * 10 + 4 + 8;
}

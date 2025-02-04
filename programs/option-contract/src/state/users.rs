use anchor_lang::prelude::*;

#[account]
pub struct Users {
    pub user_count: u64,
    pub max_count: u64,
    pub admin: Pubkey,
    #[max_len(10, 32)]
    pub users: Vec<Pubkey>, // add resize for size
}

impl Users {
    pub const LEN: usize = 8 + 8 + 32 + 32 * 10 + 4 + 8;
}

use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Users {
    pub user_count: u64,
    pub admin: Pubkey,
    pub max_count:u64,
    #[max_len(10)]
    pub users: Vec<Pubkey>, // add resize for size
}

impl Users {
    pub const LEN: usize = 8 + Users::INIT_SPACE;
}

use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct User {
    pub option_index: u64,
    pub max_index: u64,
    #[max_len(10)]
    pub options: Vec<u64>, // add resize for size
}

impl User {
    pub const LEN: usize = 8 + User::INIT_SPACE;
}

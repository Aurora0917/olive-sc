use anchor_lang::prelude::*;

#[account]
pub struct User {
    pub max_index: u64,
    #[max_len(10, 8)]
    pub options: Vec<u64>, // add resize for size
}

impl User {
    pub const LEN: usize = 8 + 80 + 4 + 8;
}

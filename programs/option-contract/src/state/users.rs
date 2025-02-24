use anchor_lang::prelude::*;

#[account]
pub struct Users {
    pub admin: Pubkey,
}

impl Users {
    pub const LEN: usize = 32 + 8;
}

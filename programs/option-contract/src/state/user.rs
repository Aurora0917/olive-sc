use anchor_lang::prelude::*;

#[account]
pub struct User {
    pub option_index: u64,           // Next option index to assign (0, 1, 2, ...)
    pub bump: u8,
    pub perp_position_index: u64,    // Next perp position index to assign
    pub future_index: u64,           // Next future index to assign
}

impl User {
    pub const LEN: usize = 8 + 1 + 8 + 8 + 8;  // option_index + bump + perp_position_index + future_index
}

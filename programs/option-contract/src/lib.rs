use anchor_lang::prelude::*;
use instructions::*;

pub mod errors;
pub mod instructions;
pub mod state;
pub mod utils;

declare_id!("DYTHL9fkyWvVEMUPeUZWqVtDMNv8joYdvTD21UWhKkeN");

#[program]
pub mod option_contract {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        instructions::initialize::initialize(ctx)
    }
}

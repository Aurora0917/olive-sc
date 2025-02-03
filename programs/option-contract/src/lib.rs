use anchor_lang::prelude::*;
use instructions::*;

pub mod instructions;
declare_id!("DYTHL9fkyWvVEMUPeUZWqVtDMNv8joYdvTD21UWhKkeN");

#[program]
pub mod option_contract {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        instructions::initialize::initialize(ctx)
    }
}
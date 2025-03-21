use anchor_lang::prelude::*;

use crate::math;

#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct Assets {
    // Total Assets
    pub owned: u64,
    // Locked assets for options
    pub locked: u64,
}

#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct Fees {
    // fees have implied BPS_DECIMALS decimals
    pub ratio_mult: u64,
    pub add_liquidity: u64,
    pub remove_liquidity: u64,
    pub close_position: u64,
}

#[account]
#[derive(Default, Debug, PartialEq)]
pub struct Custody {
    // static parameters
    pub pool: Pubkey,
    pub mint: Pubkey,
    pub token_account: Pubkey,
    pub decimals: u8,
    pub oracle: Pubkey,
    pub assets: Assets,
    pub fees: Fees, // Maintaining token ratio constant
    // bumps for address validation
    pub bump: u8,
    pub token_account_bump: u8,
}

impl Custody {
    pub const LEN: usize = 8 + std::mem::size_of::<Custody>();

    pub fn validate(&self) -> bool {
        self.token_account != Pubkey::default()
            && self.mint != Pubkey::default()
            && self.oracle != Pubkey::default()
    }

    pub fn lock_funds(&mut self, amount: u64) -> Result<()> {
        self.assets.locked = math::checked_add(self.assets.locked, amount)?;
        if self.assets.owned < self.assets.locked {
            Err(ProgramError::InsufficientFunds.into())
        } else {
            Ok(())
        }
    }

    pub fn unlock_funds(&mut self, amount: u64) -> Result<()> {
        if amount > self.assets.locked {
            self.assets.locked = 0;
        } else {
            self.assets.locked = math::checked_sub(self.assets.locked, amount)?;
        }

        Ok(())
    }
}

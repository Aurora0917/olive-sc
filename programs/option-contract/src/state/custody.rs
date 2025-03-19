use anchor_lang::prelude::*;

use crate::math;

#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct Assets {
    // Total Assets
    pub owned: u64,
    // Locked assets for options
    pub locked: u64,
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
    // pub pricing: PricingParams,
    // pub permissions: Permissions,
    // pub fees: Fees,
    // pub borrow_rate: BorrowRateParams,

    // dynamic variables
    pub assets: Assets,
    // pub collected_fees: FeesStats,
    // pub volume_stats: VolumeStats,
    // pub trade_stats: TradeStats,
    // pub long_positions: PositionStats,
    // pub short_positions: PositionStats,
    // pub borrow_rate_state: BorrowRateState,

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
        // && self.oracle.validate()
        // && self.pricing.validate()
        // && self.fees.validate()
        // && self.borrow_rate.validate()
    }

    pub fn lock_funds(&mut self, amount:u64) -> Result<()> {
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

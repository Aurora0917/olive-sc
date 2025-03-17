use anchor_lang::prelude::*;

#[account]
#[derive(Default, Debug)]
pub struct Contract {
    pub pools: Vec<Pubkey>,
    pub bump: u8,
}

impl anchor_lang::Id for Contract {
    fn id() -> Pubkey {
        crate::ID
    }
}

impl Contract {
    pub const LEN: usize = 8 + std::mem::size_of::<Contract>();
    pub const BPS_DECIMALS: u8 = 4;
    pub const BPS_POWER: u128 = 10u64.pow(Self::BPS_DECIMALS as u32) as u128;
    pub const USD_DECIMALS:i32 = 6;
    pub fn is_empty_account(account_info: &AccountInfo) -> Result<bool> {
        Ok(account_info.try_data_is_empty()? || account_info.try_lamports()? == 0)
    }
}

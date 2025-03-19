use anchor_lang::prelude::*;

#[account]
#[derive(Default, Debug)]
pub struct Contract {
    pub pools: Vec<Pubkey>,
    pub bump: u8,
    pub transfer_authority_bump:u8
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
    pub const PRICE_DECIMALS:i32 =6;
    pub fn is_empty_account(account_info: &AccountInfo) -> Result<bool> {
        Ok(account_info.try_data_is_empty()? || account_info.try_lamports()? == 0)
    }


    pub fn close_token_account<'info>(
        receiver: AccountInfo<'info>,
        token_account: AccountInfo<'info>,
        token_program: AccountInfo<'info>,
        authority: AccountInfo<'info>,
        seeds: &[&[&[u8]]],
    ) -> Result<()> {
        let cpi_accounts = anchor_spl::token::CloseAccount {
            account: token_account,
            destination: receiver,
            authority,
        };
        let cpi_context = anchor_lang::context::CpiContext::new(token_program, cpi_accounts);

        anchor_spl::token::close_account(cpi_context.with_signer(seeds))
    }

    pub fn get_time(&self) -> Result<i64> {
        let current_timestamp = Clock::get().unwrap().unix_timestamp;
        if current_timestamp > 0 {
            Ok(current_timestamp)
        } else {
            Err(ProgramError::InvalidAccountData.into())
        }
    }
}

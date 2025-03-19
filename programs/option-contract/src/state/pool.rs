use anchor_lang::prelude::*;

use crate::{errors::PoolError, math};

use super::{Custody, OraclePrice};

#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Debug)]
pub enum Side {
    None,
    Call,
    Put,
}

#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct TokenRatios {
    pub target: u64,
    pub min: u64,
    pub max: u64,
}

#[account]
#[derive(Default, Debug)]
pub struct Pool {
    pub name: String,
    pub custodies: Vec<Pubkey>,
    pub ratios: Vec<TokenRatios>,
    pub aum_usd: u128,
    pub bump: u8,
}

impl Pool {
    pub const LEN: usize = 8 + 64 + std::mem::size_of::<Pool>();

    pub fn get_token_id(&self, custody: &Pubkey) -> Result<usize> {
        self.custodies
            .iter()
            .position(|&k| k == *custody)
            .ok_or_else(|| PoolError::InvalidCustodyTokenError.into())
    }

    pub fn check_token_ratio(
        &self,
        token_id: usize,
        amount_add: u64,
        amount_remove: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<bool> {
        let new_ratio = self.get_new_ratio(amount_add, amount_remove, custody, token_price)?;

        if new_ratio < self.ratios[token_id].min {
            Ok(new_ratio >= self.get_current_ratio(custody, token_price)?)
        } else if new_ratio > self.ratios[token_id].max {
            Ok(new_ratio <= self.get_current_ratio(custody, token_price)?)
        } else {
            Ok(true)
        }
    }

    fn get_current_ratio(&self, custody: &Custody, token_price: &OraclePrice) -> Result<u64> {
        let ratio = math::checked_as_u64(math::checked_div(
            token_price.get_asset_amount_usd(custody.assets.owned, custody.decimals)? as u128,
            self.aum_usd,
        )?)?;
        Ok(ratio)
    }

    fn get_new_ratio(
        &self,
        amount_add: u64,
        amount_remove: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<u64> {
        let (new_token_aum_usd, new_pool_aum_usd) = if amount_add > 0 && amount_remove > 0 {
            return Err(ProgramError::InvalidArgument.into());
        } else if amount_add == 0 && amount_remove == 0 {
            (
                token_price.get_asset_amount_usd(custody.assets.owned, custody.decimals)? as u128,
                self.aum_usd,
            )
        } else if amount_add > 0 {
            let added_aum_usd =
                token_price.get_asset_amount_usd(amount_add, custody.decimals)? as u128;

            (
                token_price.get_asset_amount_usd(
                    math::checked_add(custody.assets.owned, amount_add)?,
                    custody.decimals,
                )? as u128,
                math::checked_add(self.aum_usd, added_aum_usd)?,
            )
        } else {
            let removed_aum_usd =
                token_price.get_asset_amount_usd(amount_remove, custody.decimals)? as u128;

            if removed_aum_usd >= self.aum_usd || amount_remove >= custody.assets.owned {
                (0, 0)
            } else {
                (
                    token_price.get_asset_amount_usd(
                        math::checked_sub(custody.assets.owned, amount_remove)?,
                        custody.decimals,
                    )? as u128,
                    math::checked_sub(self.aum_usd, removed_aum_usd)?,
                )
            }
        };
        if new_token_aum_usd == 0 || new_pool_aum_usd == 0 {
            return Ok(0);
        }

        let ratio = math::checked_as_u64(math::checked_div(new_token_aum_usd, new_pool_aum_usd)?)?;
        Ok(ratio)
    }
}

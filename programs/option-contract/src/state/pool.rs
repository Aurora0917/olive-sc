use std::cmp::Ordering;

use anchor_lang::prelude::*;

use crate::{errors::PoolError, math, utils::{BorrowRateCurve, Fraction}};

use super::{Contract, Custody, OraclePrice};

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
    pub lp_token_bump: u8,
    
    // Borrow/Funding rate curve for dynamic rate calculation
    pub borrow_rate_curve: BorrowRateCurve,
    
    // Cumulative rates for position tracking
    pub cumulative_funding_rate_long: i128,
    pub cumulative_funding_rate_short: i128,
    pub cumulative_interest_rate: u128,
    pub last_rate_update: i64,
    
    // Open interest tracking for perpetuals
    pub long_open_interest_usd: u128,
    pub short_open_interest_usd: u128,
    
    // Pool utilization tracking
    pub total_borrowed_usd: u128,
    pub last_utilization_update: i64,
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
        if self.aum_usd == 0 {
            Ok(0)
        } else {
            let ratio = math::checked_as_u64(math::checked_div(
                math::checked_mul(token_price.get_asset_amount_usd(custody.token_owned, custody.decimals)? as u128, 100)?,
                self.aum_usd,
            )?)?;
            Ok(ratio)
        }
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
                token_price.get_asset_amount_usd(custody.token_owned, custody.decimals)? as u128,
                self.aum_usd,
            )
        } else if amount_add > 0 {
            let added_aum_usd =
                token_price.get_asset_amount_usd(amount_add, custody.decimals)? as u128;
            msg!("amount_add: {}", amount_add);
            msg!("custody.decimals: {}", custody.decimals);
            msg!("token_price.price: {}", token_price.price);
            msg!("added_aum_usd: {}", added_aum_usd);

            (
                token_price.get_asset_amount_usd(
                    math::checked_add(custody.token_owned, amount_add)?,
                    custody.decimals,
                )? as u128,
                math::checked_add(self.aum_usd, added_aum_usd)?,
            )
        } else {
            let removed_aum_usd =
                token_price.get_asset_amount_usd(amount_remove, custody.decimals)? as u128;

            if removed_aum_usd >= self.aum_usd || amount_remove >= custody.token_owned {
                (0, 0)
            } else {
                (
                    token_price.get_asset_amount_usd(
                        math::checked_sub(custody.token_owned, amount_remove)?,
                        custody.decimals,
                    )? as u128,
                    math::checked_sub(self.aum_usd, removed_aum_usd)?,
                )
            }
        };
        if new_token_aum_usd == 0 || new_pool_aum_usd == 0 {
            return Ok(0);
        }

        msg!("new_token_aum_usd: {}", new_token_aum_usd);
        msg!("new_pool_aum_usd: {}", new_pool_aum_usd);

        let ratio = math::checked_as_u64(math::checked_div(new_token_aum_usd * 100, new_pool_aum_usd)?)?;
        Ok(ratio)
    }

    pub fn check_available_amount(&self, amount: u64, custody: &Custody) -> Result<bool> {
        let available_amount = math::checked_sub(custody.token_owned, custody.token_locked)?;
        Ok(available_amount >= amount)
    }

    // Calculate Pool AUM
    pub fn get_assets_under_management_usd<'info>(
        &self,
        accounts: &'info [AccountInfo<'info>],
        curtime: i64,
    ) -> Result<u128> {
        let mut pool_amount_usd: u128 = 0;
        for (idx, &custody) in self.custodies.iter().enumerate() {
            let oracle_idx = idx + self.custodies.len();
            if oracle_idx >= accounts.len() {
                return Err(ProgramError::NotEnoughAccountKeys.into());
            }
            let custody_info = &accounts[idx];
            require_keys_eq!(accounts[idx].key(), custody);
            let custody = Account::<Custody>::try_from(custody_info)?;

            require_keys_eq!(accounts[oracle_idx].key(), custody.oracle);

            let token_price = OraclePrice::new_from_oracle(&accounts[oracle_idx], curtime, false)?;
            let token_amount_usd =
                token_price.get_asset_amount_usd(custody.token_owned, custody.decimals)?;
            msg!("token_amount_usd: {}", token_amount_usd);
            msg!("token_price: {}", token_price.price);
            msg!("custody.token_owned: {}", custody.token_owned);
            msg!("custody.decimals: {}", custody.decimals);
            
            pool_amount_usd = math::checked_add(pool_amount_usd, token_amount_usd as u128)?;
            msg!("pool_amount_usd: {}", pool_amount_usd);
        }

        Ok(pool_amount_usd)
    }

    pub fn get_add_liquidity_fee(
        &self,
        token_id: usize,
        amount: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<u64> {
        self.get_fee(
            token_id,
            custody.fees.add_liquidity,
            amount,
            0u64,
            custody,
            token_price,
        )
    }

    fn get_fee(
        &self,
        token_id: usize,
        base_fee: u64,
        amount_add: u64,
        amount_remove: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<u64> {
        self.get_fee_linear(
            token_id,
            base_fee,
            amount_add,
            amount_remove,
            custody,
            token_price,
        )
    }

    fn get_fee_linear(
        &self,
        token_id: usize,
        base_fee: u64,
        amount_add: u64,
        amount_remove: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<u64> {
        // if token ratio is improved:
        //    fee = base_fee / ratio_fee
        // otherwise:
        //    fee = base_fee * ratio_fee
        // where:
        //   if new_ratio < ratios.target:
        //     ratio_fee = 1 + custody.fees.ratio_mult * (ratios.target - new_ratio) / (ratios.target - ratios.min);
        //   otherwise:
        //     ratio_fee = 1 + custody.fees.ratio_mult * (new_ratio - ratios.target) / (ratios.max - ratios.target);

        let ratios = &self.ratios[token_id];
        let current_ratio = self.get_current_ratio(custody, token_price)?;
        let new_ratio = self.get_new_ratio(amount_add, amount_remove, custody, token_price)?;

        msg!("current_ratio: {}", current_ratio);

        let improved = match new_ratio.cmp(&ratios.target) {
            Ordering::Less => {
                new_ratio > current_ratio
                    || (current_ratio > ratios.target
                        && current_ratio - ratios.target > ratios.target - new_ratio)
            }
            Ordering::Greater => {
                new_ratio < current_ratio
                    || (current_ratio < ratios.target
                        && ratios.target - current_ratio > new_ratio - ratios.target)
            }
            Ordering::Equal => current_ratio != ratios.target,
        };
        msg!("new_ratio: {}, ratios.target: {}", new_ratio, ratios.target);
        let ratio_fee = if new_ratio <= ratios.target {
            if ratios.target == ratios.min {
                Contract::BPS_POWER
            } else {
                math::checked_add(
                    Contract::BPS_POWER,
                    math::checked_div(
                        math::checked_mul(
                            custody.fees.ratio_mult as u128,
                            math::checked_sub(ratios.target, new_ratio)? as u128,
                        )?,
                        math::checked_sub(ratios.target, ratios.min)? as u128,
                    )?,
                )?
            }
        } else if ratios.target == ratios.max {
            Contract::BPS_POWER
        } else {
            math::checked_add(
                Contract::BPS_POWER,
                math::checked_div(
                    math::checked_mul(
                        custody.fees.ratio_mult as u128,
                        math::checked_sub(new_ratio, ratios.target)? as u128,
                    )?,
                    math::checked_sub(ratios.max, ratios.target)? as u128,
                )?,
            )?
        };
        msg!("ratio_fee: {}", ratio_fee);
        let fee = if improved {
            math::checked_div(
                math::checked_mul(base_fee as u128, Contract::BPS_POWER)?,
                ratio_fee,
            )?
        } else {
            math::checked_div(
                math::checked_mul(base_fee as u128, ratio_fee)?,
                Contract::BPS_POWER,
            )?
        };
        msg!("fee: {}", fee);

        Self::get_fee_amount(
            math::checked_as_u64(fee)?,
            std::cmp::max(amount_add, amount_remove),
        )
    }

    pub fn get_fee_amount(fee: u64, amount: u64) -> Result<u64> {
        if fee == 0 || amount == 0 {
            return Ok(0);
        }
        math::checked_as_u64(math::checked_ceil_div(
            math::checked_mul(amount as u128, fee as u128)?,
            Contract::BPS_POWER,
        )?)
    }

    pub fn get_remove_liquidity_fee(
        &self,
        token_id: usize,
        amount: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<u64> {
        self.get_fee(
            token_id,
            custody.fees.remove_liquidity,
            0u64,
            amount,
            custody,
            token_price,
        )
    }

    // Calculate current pool utilization based on borrowed funds vs available funds
    pub fn calculate_utilization(&self, custodies: &[Custody]) -> Result<Fraction> {
        let mut total_liquidity_usd = 0u128;
        let mut total_borrowed_usd = 0u128;
        
        for custody in custodies {
            // Calculate available liquidity (owned - locked)
            let available_amount = math::checked_sub(custody.token_owned, custody.token_locked)?;
            
            // For simplicity, assume 1:1 USD ratio for now
            // In production, this should use oracle prices
            total_liquidity_usd = math::checked_add(total_liquidity_usd, available_amount as u128)?;
            total_borrowed_usd = math::checked_add(total_borrowed_usd, custody.token_locked as u128)?;
        }
        
        if total_liquidity_usd == 0 {
            return Ok(Fraction::ZERO);
        }
        
        let utilization_bps = math::checked_div(
            math::checked_mul(total_borrowed_usd, 10000)?, // Convert to basis points
            total_liquidity_usd
        )? as u32;
        Ok(Fraction::from_bps(utilization_bps.min(10000)))
    }
    
    // Update borrow rates and cumulative rates using the borrow rate curve
    pub fn update_rates(&mut self, current_time: i64, custodies: &[Custody]) -> Result<()> {
        if current_time <= self.last_rate_update {
            return Ok(());
        }
        
        let time_delta = current_time - self.last_rate_update;
        
        // Calculate current utilization
        let utilization = self.calculate_utilization(custodies)?;
        
        // Get current borrow rate from curve
        let current_borrow_rate = self.borrow_rate_curve.get_borrow_rate(utilization)?;
        
        // Calculate funding rate based on open interest imbalance
        let total_oi = math::checked_add(self.long_open_interest_usd, self.short_open_interest_usd)?;
        let funding_rate = if total_oi > 0 {
            let imbalance_ratio = if self.long_open_interest_usd > self.short_open_interest_usd {
                math::checked_div(
                    math::checked_sub(self.long_open_interest_usd, self.short_open_interest_usd)?,
                    total_oi
                )?
            } else {
                math::checked_div(
                    math::checked_sub(self.short_open_interest_usd, self.long_open_interest_usd)?,
                    total_oi
                )?
            };
            
            // Base funding rate with imbalance modifier
            let base_rate = current_borrow_rate.to_bps().unwrap_or(0u32) as i128;
            let imbalance_modifier = (imbalance_ratio as i128) / 100; // Scale imbalance
            
            if self.long_open_interest_usd > self.short_open_interest_usd {
                base_rate + imbalance_modifier
            } else {
                base_rate - imbalance_modifier
            }
        } else {
            0i128
        };
        
        // Calculate time-based increments (per hour)
        let hours_elapsed = math::checked_div(time_delta as u128, 3600)?; // 3600 seconds per hour
        if hours_elapsed == 0 {
            return Ok(());
        }
        
        // Calculate hourly rate increments
        let borrow_rate_bps = current_borrow_rate.to_bps().unwrap_or(0u32) as u128;
        let hourly_borrow_increment = math::checked_div(
            math::checked_mul(borrow_rate_bps, hours_elapsed)?,
            8760 // Hours in a year
        )?;
        
        let hourly_funding_increment = math::checked_div(
            math::checked_mul(funding_rate.abs() as u128, hours_elapsed)?,
            8760 * 3 // Funding paid every 8 hours
        )? as i128;
        
        // Update cumulative rates
        self.cumulative_interest_rate = math::checked_add(
            self.cumulative_interest_rate,
            hourly_borrow_increment
        )?;
        
        if funding_rate >= 0 {
            // Longs pay shorts
            self.cumulative_funding_rate_long = math::checked_sub(
                self.cumulative_funding_rate_long,
                hourly_funding_increment
            )?;
            self.cumulative_funding_rate_short = math::checked_add(
                self.cumulative_funding_rate_short,
                hourly_funding_increment
            )?;
        } else {
            // Shorts pay longs
            self.cumulative_funding_rate_long = math::checked_add(
                self.cumulative_funding_rate_long,
                hourly_funding_increment
            )?;
            self.cumulative_funding_rate_short = math::checked_sub(
                self.cumulative_funding_rate_short,
                hourly_funding_increment
            )?;
        }
        
        self.last_rate_update = current_time;
        Ok(())
    }
    
    // Get current borrow rate based on utilization
    pub fn get_current_borrow_rate(&self, custodies: &[Custody]) -> Result<Fraction> {
        let utilization = self.calculate_utilization(custodies)?;
        self.borrow_rate_curve.get_borrow_rate(utilization)
    }
    
    // Get funding payment for a position based on cumulative rates
    pub fn get_funding_payment(&self, position_side: bool, position_size_usd: u128, cumulative_funding_snapshot: i128) -> Result<i128> {
        let current_cumulative = if position_side {
            self.cumulative_funding_rate_long
        } else {
            self.cumulative_funding_rate_short
        };
        
        let funding_rate_delta = math::checked_sub(current_cumulative, cumulative_funding_snapshot)?;
        
        // Calculate funding payment: position_size * funding_rate_delta / 10000 (bps)
        let funding_payment = math::checked_div(
            math::checked_mul(position_size_usd as i128, funding_rate_delta)?,
            10000
        )?;
        
        Ok(funding_payment)
    }
    
    // Get interest payment for borrowed funds
    pub fn get_interest_payment(&self, borrow_size_usd: u128, cumulative_interest_snapshot: u128) -> Result<u128> {
        let interest_rate_delta = math::checked_sub(
            self.cumulative_interest_rate,
            cumulative_interest_snapshot
        )?;
        
        // Calculate interest payment: borrow_size * interest_rate_delta / 10000 (bps)
        let interest_payment = math::checked_div(
            math::checked_mul(borrow_size_usd, interest_rate_delta)?,
            10000
        )?;
        
        Ok(interest_payment)
    }
    
    // Get current open interest for funding rate calculation
    pub fn get_open_interest_usd(&self) -> Result<(u128, u128)> {
        Ok((self.long_open_interest_usd, self.short_open_interest_usd))
    }
    
    // Initialize the pool with a default borrow rate curve
    pub fn initialize_borrow_rate_curve(&mut self) -> Result<()> {
        // Set up a reasonable default curve:
        // 0% utilization: 2% APR
        // 80% utilization: 10% APR  
        // 100% utilization: 30% APR
        self.borrow_rate_curve = BorrowRateCurve::from_legacy_parameters(
            80,  // optimal_utilization_rate_pct
            2,   // base_rate_pct
            10,  // optimal_rate_pct
            30,  // max_rate_pct
        );
        Ok(())
    }
}

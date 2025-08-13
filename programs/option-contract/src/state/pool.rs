use std::cmp::Ordering;

use anchor_lang::prelude::*;

use crate::{errors::PoolError, math, utils::{self, BorrowRateCurve, Fraction}};

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
    
    // Cumulative borrow interest rates for position tracking
    pub cumulative_interest_rate_long: u128,
    pub cumulative_interest_rate_short: u128,
    pub last_rate_update: i64,
    
    // Open interest tracking for perpetuals
    pub long_open_interest_usd: u128,
    pub short_open_interest_usd: u128,
    
    // Pool utilization tracking
    pub total_borrowed_usd: u128,
    pub last_utilization_update: i64,
    
    // Fixed rate tracking for futures and options (2D utilization)
    pub total_future_notional_usd: u128,      // Total USD value of all open futures
    pub total_future_time_value: u128,        // Sum of (notional * time_to_expiry) for all futures
    pub total_option_notional_usd: u128,      // Total USD value of all open options
    pub total_option_time_value: u128,        // Sum of (notional * time_to_expiry) for all options
    pub last_fixed_rate_update: i64,          // Last time fixed rates were updated
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

    // Calculate per-token utilization and borrow rate
    pub fn get_token_borrow_rate(&self, custody: &Custody) -> Result<Fraction> {
        if custody.token_owned == 0 {
            return Ok(Fraction::ZERO);
        }
        
        // Calculate per-token utilization: token_locked / token_owned
        let token_utilization_pct = utils::pool::calculate_utilization(custody.token_locked, custody.token_owned);
        let token_utilization_bps = (token_utilization_pct * 100.0) as u32;
        let utilization = Fraction::from_bps(token_utilization_bps.min(10000));
        
        // Get borrow rate from curve for this specific token
        self.borrow_rate_curve.get_borrow_rate(utilization)
    }
    
    // Update position borrow fees before any position modification
    pub fn update_position_borrow_fees(
        &self,
        position: &mut crate::state::Position,
        current_time: i64,
        sol_custody: &Custody,
        usdc_custody: &Custody,
    ) -> Result<u64> {
        // Limit orders don't pay borrow fees until executed
        if position.order_type == crate::state::OrderType::Limit {
            return Ok(0);
        }
        
        // Determine which custody to use based on position side
        let relevant_custody = match position.side {
            crate::state::Side::Long => sol_custody, // Long positions borrow SOL
            crate::state::Side::Short => usdc_custody, // Short positions borrow USDC
        };
        
        // Get current borrow rate for time-based calculation
        let current_borrow_rate = self.get_token_borrow_rate(relevant_custody)?;
        let current_borrow_rate_bps = current_borrow_rate.to_bps().unwrap_or(0u32);
        
        // Calculate and accrue time-based borrow fees
        position.calculate_and_accrue_borrow_fees(current_time, current_borrow_rate_bps)
    }

    
    // Get current borrow rate for a specific custody token
    pub fn get_current_borrow_rate(&self, custody: &Custody) -> Result<Fraction> {
        self.get_token_borrow_rate(custody)
    }
    
    // Get current open interest 
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

    /// Calculate 2D utilization based on both current usage and time commitment
    /// Formula: liquidity_time_taken / (TVL * 365 days)
    pub fn calculate_2d_utilization(&self, _current_time: i64) -> Result<u64> {
        // Total possible liquidity-time (AUM * 365 days in seconds)
        let seconds_per_year = 365u128 * 24 * 3600; // 31,536,000 seconds
        let total_possible = math::checked_mul(self.aum_usd, seconds_per_year)?;
        
        if total_possible == 0 {
            return Ok(0);
        }
        
        // Current liquidity-time taken by futures and options
        let total_time_value = math::checked_add(
            self.total_future_time_value,
            self.total_option_time_value
        )?;
        
        // Calculate 2D utilization as percentage in basis points
        let utilization_2d = math::checked_div(
            math::checked_mul(total_time_value, 10_000u128)?,
            total_possible
        )?;
        
        Ok(math::checked_as_u64(utilization_2d.min(10_000))?)
    }
    
    /// Calculate fixed interest rate for new futures/options using 2D utilization
    pub fn calculate_fixed_interest_rate(&self, _current_time: i64) -> Result<u32> {
        // Get current variable rate (base rate from borrow curve - first point)
        let base_rate_bps = self.borrow_rate_curve.points[0].borrow_rate_bps;
        
        // Get 2D utilization
        let utilization_2d_bps = self.calculate_2d_utilization(_current_time)?;
        
        // Calculate premium based on 2D utilization curve
        let fixed_rate_premium = self.calculate_fixed_rate_premium(utilization_2d_bps)?;
        
        // Fixed rate = base rate + premium based on 2D utilization
        Ok(math::checked_add(base_rate_bps, fixed_rate_premium)?)
    }
    
    /// Calculate premium for fixed rates based on 2D utilization
    /// Implements exponential curve similar to Aave's stable rate mechanism
    fn calculate_fixed_rate_premium(&self, utilization_2d_bps: u64) -> Result<u32> {
        // Progressive rate increases based on 2D utilization
        // This prevents exploitation while keeping rates reasonable
        
        match utilization_2d_bps {
            0..=2000 => Ok(0),       // 0-20%: no premium
            2001..=4000 => Ok(50),   // 20-40%: 0.5% premium
            4001..=6000 => Ok(150),  // 40-60%: 1.5% premium
            6001..=8000 => Ok(400),  // 60-80%: 4% premium
            8001..=9000 => Ok(800),  // 80-90%: 8% premium
            9001..=9500 => Ok(1500), // 90-95%: 15% premium
            9501..=9800 => Ok(3000), // 95-98%: 30% premium
            _ => Ok(5000),           // 98%+: 50% premium (very high to discourage)
        }
    }
    
    /// Add future position to pool tracking
    pub fn add_future_position(
        &mut self,
        notional_usd: u64,
        time_to_expiry_seconds: i64,
        current_time: i64,
    ) -> Result<u32> {
        // Calculate and lock in the fixed rate
        let fixed_rate_bps = self.calculate_fixed_interest_rate(current_time)?;
        
        // Update pool tracking
        self.total_future_notional_usd = math::checked_add(
            self.total_future_notional_usd,
            notional_usd as u128
        )?;
        
        let time_value = math::checked_mul(
            notional_usd as u128,
            time_to_expiry_seconds as u128
        )?;
        
        self.total_future_time_value = math::checked_add(
            self.total_future_time_value,
            time_value
        )?;
        
        self.last_fixed_rate_update = current_time;
        
        Ok(fixed_rate_bps)
    }
    
    /// Remove future position from pool tracking
    pub fn remove_future_position(
        &mut self,
        notional_usd: u64,
        time_to_expiry_seconds: i64,
        current_time: i64,
    ) -> Result<()> {
        // Remove from pool tracking
        self.total_future_notional_usd = self.total_future_notional_usd
            .saturating_sub(notional_usd as u128);
        
        let time_value = math::checked_mul(
            notional_usd as u128,
            time_to_expiry_seconds as u128
        )?;
        
        self.total_future_time_value = self.total_future_time_value
            .saturating_sub(time_value);
        
        self.last_fixed_rate_update = current_time;
        
        Ok(())
    }
    
    /// Add option position to pool tracking (for future use)
    pub fn add_option_position(
        &mut self,
        notional_usd: u64,
        time_to_expiry_seconds: i64,
        current_time: i64,
    ) -> Result<u32> {
        let fixed_rate_bps = self.calculate_fixed_interest_rate(current_time)?;
        
        self.total_option_notional_usd = math::checked_add(
            self.total_option_notional_usd,
            notional_usd as u128
        )?;
        
        let time_value = math::checked_mul(
            notional_usd as u128,
            time_to_expiry_seconds as u128
        )?;
        
        self.total_option_time_value = math::checked_add(
            self.total_option_time_value,
            time_value
        )?;
        
        self.last_fixed_rate_update = current_time;
        
        Ok(fixed_rate_bps)
    }
    
    /// Remove option position from pool tracking (for future use)
    pub fn remove_option_position(
        &mut self,
        notional_usd: u64,
        time_to_expiry_seconds: i64,
        current_time: i64,
    ) -> Result<()> {
        self.total_option_notional_usd = self.total_option_notional_usd
            .saturating_sub(notional_usd as u128);
        
        let time_value = math::checked_mul(
            notional_usd as u128,
            time_to_expiry_seconds as u128
        )?;
        
        self.total_option_time_value = self.total_option_time_value
            .saturating_sub(time_value);
        
        self.last_fixed_rate_update = current_time;
        
        Ok(())
    }
}

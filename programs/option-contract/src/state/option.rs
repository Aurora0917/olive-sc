use anchor_lang::prelude::*;
use crate::{utils::borrow_rate_curve::*, utils::Fraction, math::{self, scaled_price_to_f64}};

#[account]
pub struct OptionDetail {
    pub index: u64,
    pub owner: Pubkey,
    pub amount: u64,
    pub quantity: u64,
    pub strike_price: u64,        // Strike price scaled by 1e6 (6 decimals)
    pub period: u64,
    pub expired_date: i64,
    pub purchase_date: u64,
    pub option_type: u8, // 0: call, 1: put

    pub premium: u64,
    pub premium_asset: Pubkey, // pay_custody key
    pub profit: u64,
    pub locked_asset: Pubkey, // locked custody key

    pub pool: Pubkey,
    pub custody: Pubkey,

    pub exercised: u64,
    pub bought_back: u64, // time Stamp when
    pub claimed: u64,     // claimable amount after automatically exercise by bot.
    pub valid: bool,      // false - invalid/expired/exercised, true - valid
    pub bump: u8,
    pub limit_price: u64,
    pub executed: bool,
    
    // NEW FIELD
    pub entry_price: u64,     // Underlying asset price when option was purchased (scaled by 1e6)
    pub last_update_time: i64, // Last time option was updated
}

impl OptionDetail {
    // Updated length calculation: added 8 bytes for entry_price (u64) + 8 bytes for last_update_time (i64)
    pub const LEN: usize = 8 * 15 + 4 + 32 * 5 + 8;

    pub fn normal_cdf(z: f64) -> f64 {
        let beta1 = -0.0004406;
        let beta2 = 0.0418198;
        let beta3 = 0.9;
        let exponent =
            -std::f64::consts::PI.sqrt() * (beta1 * z.powi(5) + beta2 * z.powi(3) + beta3 * z);
        1.0 / (1.0 + exponent.exp())
    }
    
    pub fn black_scholes(
        s: f64,
        k: f64,
        t: f64,
        call: bool, // true : call , false : put
    ) -> f64 {
        let r = 0.0;
        let sigma = 0.5;
        let d1 = ((s / k).ln() + (r + 0.5 * sigma * sigma) * t) / (sigma * t.sqrt());
        let d2 = d1 - sigma * t.sqrt();
    
        let n_d1 = OptionDetail::normal_cdf(d1);
        let n_d2 = OptionDetail::normal_cdf(d2);
        let n_neg_d1 = OptionDetail::normal_cdf(-d1);
        let n_neg_d2 = OptionDetail::normal_cdf(-d2);
    
        if call {
            s * n_d1 - k * (-r * t).exp() * n_d2
        } else {
            k * (-r * t).exp() * n_neg_d2 - s * n_neg_d1
        }
    }

    /// Calculate utilization percentage: (tokenLocked / tokenOwned) * 100
    pub fn calculate_utilization(token_locked: u64, token_owned: u64) -> f64 {
        if token_owned == 0 {
            0.0
        } else {
            ((token_locked as f64 / token_owned as f64) * 100.0).round()
        }
    }

    /// Calculate borrow rate using 11-point curve
    pub fn calculate_borrow_rate(token_locked: u64, token_owned: u64, is_sol: bool) -> Result<f64> {
        // Get the appropriate 11-point curve
        let curve = if is_sol {
            // SOL: 3% base, 12% optimal at 75%, 60% max
            BorrowRateCurve::from_legacy_parameters(80, 3, 12, 60)
        } else {
            // USDC: 1% base, 5% optimal at 85%, 25% max
            BorrowRateCurve::from_legacy_parameters(80, 1, 5, 25)
        };

        // Calculate utilization
        let utilization_pct = Self::calculate_utilization(token_locked, token_owned);
        let utilization_bps = (utilization_pct * 100.0) as u32;
        let utilization_fraction = Fraction::from_bps(utilization_bps.min(10000));

        // Get borrow rate from curve
        let borrow_rate_fraction = curve.get_borrow_rate(utilization_fraction)?;
        let borrow_rate_bps = borrow_rate_fraction.to_bps().unwrap();

        // Convert to percentage
        Ok(borrow_rate_bps as f64 / 100.0)
    }

    /// Get SOL borrow rate
    pub fn get_sol_borrow_rate(sol_locked: u64, sol_owned: u64) -> Result<f64> {
        Self::calculate_borrow_rate(sol_locked, sol_owned, true)
    }

    /// Get USDC borrow rate  
    pub fn get_usdc_borrow_rate(usdc_locked: u64, usdc_owned: u64) -> Result<f64> {
        Self::calculate_borrow_rate(usdc_locked, usdc_owned, false)
    }
    
    /// Enhanced Black-Scholes with dynamic risk-free rate from borrow curves
    pub fn black_scholes_with_borrow_rate(
        s: f64,               // Current price
        k: f64,               // Strike price  
        t: f64,               // Time to expiration
        call: bool,           // Option type
        token_locked: u64,    // Current locked tokens
        token_owned: u64,     // Total owned tokens
        is_sol: bool,         // Asset type
    ) -> Result<f64> {
        // Calculate dynamic risk-free rate from borrow curve
        let r = Self::calculate_borrow_rate(token_locked, token_owned, is_sol)? / 100.0;
        let sigma = 0.5; // Keep volatility simple for now

        let d1 = ((s / k).ln() + (r + 0.5 * sigma * sigma) * t) / (sigma * t.sqrt());
        let d2 = d1 - sigma * t.sqrt();
    
        let n_d1 = Self::normal_cdf(d1);
        let n_d2 = Self::normal_cdf(d2);
        let n_neg_d1 = Self::normal_cdf(-d1);
        let n_neg_d2 = Self::normal_cdf(-d2);
    
        let price = if call {
            s * n_d1 - k * (-r * t).exp() * n_d2
        } else {
            k * (-r * t).exp() * n_neg_d2 - s * n_neg_d1
        };

        Ok(price)
    }

    /// Update option with current market data (similar to update_position)
    pub fn update_option(
        &mut self, 
        current_price: f64, 
        current_time: i64,
        token_locked: u64,
        token_owned: u64,
        is_sol: bool
    ) -> Result<()> {
        // Check if option is still valid
        if !self.valid || current_time > self.expired_date {
            return Ok(()); // Don't update invalid/expired options
        }

        // Calculate time to expiration in years
        let time_to_expiry = math::checked_float_div(
            (self.expired_date - current_time) as f64,
            365.25 * 24.0 * 3600.0 // seconds in a year
        )?;

        // If time to expiry is <= 0, option is expired
        if time_to_expiry <= 0.0 {
            self.valid = false;
            return Ok(());
        }

        // Calculate current option value using Black-Scholes with dynamic rates
        // Convert scaled strike price back to f64 for calculation
        let strike_price_f64 = scaled_price_to_f64(self.strike_price)?;
        let current_option_value = Self::black_scholes_with_borrow_rate(
            current_price,
            strike_price_f64,
            time_to_expiry,
            self.option_type == 0, // 0 = call, 1 = put
            token_locked,
            token_owned,
            is_sol
        )?;

        // Calculate profit/loss using proper decimal math
        // Use the same scaling as the premium asset to avoid precision loss
        let current_value_scaled = math::checked_decimal_mul(
            math::checked_as_u64(current_option_value * 1_000_000.0)?, // Convert to micro units
            -6, // micro decimals
            1,
            0,
            -6, // target 6 decimals to match common stablecoin precision
        )?;
        
        if current_value_scaled > self.premium {
            self.profit = current_value_scaled - self.premium;
        } else {
            self.profit = 0; // No negative profit, just loss
        }

        // Check if limit price conditions are met for automatic execution
        if !self.executed && self.limit_price > 0 {
            let should_execute = if self.option_type == 0 { // Call option
                current_price >= (self.limit_price as f64 / 1_000_000.0) // Assuming 6 decimal scaling
            } else { // Put option
                current_price <= (self.limit_price as f64 / 1_000_000.0)
            };

            if should_execute {
                self.executed = true;
                self.exercised = current_time as u64;
            }
        }

        // Update last update time
        self.last_update_time = current_time;

        Ok(())
    }
}
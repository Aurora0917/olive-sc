
use anchor_lang::prelude::*;
use crate::{utils::borrow_rate_curve::*, utils::Fraction};

#[account]
pub struct OptionDetail {
    pub index: u64,
    pub owner: Pubkey,
    pub amount: u64,
    pub quantity: u64,
    pub strike_price: f64,
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
}

impl OptionDetail {
    pub const LEN: usize = 8 * 13 + 1 * 4 + 32 * 5 + 8;

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
}
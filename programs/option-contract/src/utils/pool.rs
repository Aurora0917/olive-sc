use anchor_lang::prelude::*;
use crate::{utils::borrow_rate_curve::*, utils::Fraction};

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
    let utilization_pct = calculate_utilization(token_locked, token_owned);
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
    calculate_borrow_rate(sol_locked, sol_owned, true)
}

/// Get USDC borrow rate  
pub fn get_usdc_borrow_rate(usdc_locked: u64, usdc_owned: u64) -> Result<f64> {
    calculate_borrow_rate(usdc_locked, usdc_owned, false)
}


pub fn get_pool_borrow_rates(
    sol_locked: u64,
    sol_owned: u64,
    usdc_locked: u64,
    usdc_owned: u64,
) -> Result<(f64, f64)> {
    let sol_rate = get_sol_borrow_rate(sol_locked, sol_owned)?;
    let usdc_rate = get_usdc_borrow_rate(usdc_locked, usdc_owned)?;
    Ok((sol_rate, usdc_rate))
}

/// Log pool utilization and rates (for debugging)
pub fn log_pool_status(
    sol_locked: u64,
    sol_owned: u64,
    usdc_locked: u64,
    usdc_owned: u64,
) -> Result<()> {
    let sol_util = calculate_utilization(sol_locked, sol_owned);
    let usdc_util = calculate_utilization(usdc_locked, usdc_owned);
    let (sol_rate, usdc_rate) = get_pool_borrow_rates(sol_locked, sol_owned, usdc_locked, usdc_owned)?;

    msg!(
        "Pool Status - SOL: {:.0}% util, {:.2}% rate | USDC: {:.0}% util, {:.2}% rate",
        sol_util, sol_rate, usdc_util, usdc_rate
    );

    Ok(())
}

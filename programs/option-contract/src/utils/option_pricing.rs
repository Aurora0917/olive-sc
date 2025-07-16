use anchor_lang::prelude::*;
use crate::{utils::pool::*};

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

    let n_d1 = normal_cdf(d1);
    let n_d2 = normal_cdf(d2);
    let n_neg_d1 = normal_cdf(-d1);
    let n_neg_d2 = normal_cdf(-d2);

    if call {
        s * n_d1 - k * (-r * t).exp() * n_d2
    } else {
        k * (-r * t).exp() * n_neg_d2 - s * n_neg_d1
    }
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
    let r = calculate_borrow_rate(token_locked, token_owned, is_sol)? / 100.0;
    let sigma = if is_sol { 0.8 } else { 0.3 }; // Keep volatility simple for now

    let d1 = ((s / k).ln() + (r + 0.5 * sigma * sigma) * t) / (sigma * t.sqrt());
    let d2 = d1 - sigma * t.sqrt();

    let n_d1 = normal_cdf(d1);
    let n_d2 = normal_cdf(d2);
    let n_neg_d1 = normal_cdf(-d1);
    let n_neg_d2 = normal_cdf(-d2);

    let price = if call {
        s * n_d1 - k * (-r * t).exp() * n_d2
    } else {
        k * (-r * t).exp() * n_neg_d2 - s * n_neg_d1
    };

    Ok(price)
}
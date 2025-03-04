use anchor_lang::prelude::*;
// TODO: UPDATE from Devnet to Mainnet

// // Mainnet
// pub const WSOL_MINT_ADDRESS : Pubkey = pubkey!("So11111111111111111111111111111111111111112");
// pub const USDC_MINT_ADDRESS : Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
// pub const SOL_USD_PYTH_ACCOUNT : Pubkey = pubkey!("H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG");
// pub const USDC_DECIMALS: u32 = 6;
// pub const WSOL_DECIMALS: u32 = 9;

// // Devnet
pub const WSOL_MINT_ADDRESS: Pubkey = pubkey!("AvGyRAUiWkF6fzALe1LNnzCwGbNTZ4aqyfthuEZHM5Wq");
pub const USDC_MINT_ADDRESS: Pubkey = pubkey!("4dfkxzRKJzwhWHAkJErU4YVKr8RVKESDFj5xKqGuw7Xs");
pub const SOL_USD_PYTH_ACCOUNT: Pubkey = pubkey!("J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix");
pub const USDC_DECIMALS: u32 = 6;
pub const WSOL_DECIMALS: u32 = 6;

fn normal_cdf(z: f64) -> f64 {
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

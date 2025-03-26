use anchor_lang::prelude::*;

#[account]
pub struct OptionDetail {
    pub index: u64,
    pub amount: u64,
    pub strike_price: f64,
    pub period: u64,
    pub expired_date: u64,

    pub premium: u64,
    pub premium_asset: Pubkey, // pay_custody key
    pub profit: u64,
    pub locked_asset: Pubkey, // locked custody key

    pub pool : Pubkey,
    pub custody : Pubkey,

    pub exercised: u64,
    pub bought_back: u64, // time Stamp when
    pub claimed: u64,     // cliamable amount after automaticaally exercise by bot.
    pub valid: bool,      // false - invalid/expried/exercised, true - valid
    pub bump: u8,
}

impl OptionDetail {
    pub const LEN: usize = 8 * 10 + 1 + 1 + 32 * 4 + 8;

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
}

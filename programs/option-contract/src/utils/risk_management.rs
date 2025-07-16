use crate::{
    math::{self, f64_to_scaled_price},
    state::{Position, Side},
};
use anchor_lang::prelude::*;

pub fn calculate_liquidation_price(
    entry_price: u64,
    side: Side
) -> Result<u64> {
    let entry_price_f64 = math::checked_float_div(entry_price as f64, crate::math::PRICE_SCALE as f64)?;
    let margin_ratio = Position::LIQUIDATION_MARGIN_BPS as f64 / 10_000.0;
    
    let liquidation_price_f64 = match side {
        Side::Long => {
            // Long liquidation: price falls by margin ratio
            math::checked_float_mul(entry_price_f64, 1.0 - margin_ratio)?
        },
        Side::Short => {
            // Short liquidation: price rises by margin ratio
            math::checked_float_mul(entry_price_f64, 1.0 + margin_ratio)?
        }
    };
    
    f64_to_scaled_price(liquidation_price_f64)
}

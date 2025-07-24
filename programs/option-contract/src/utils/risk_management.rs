use crate::{
    errors::PerpetualError, math::{self, f64_to_scaled_price}, state::{Position, Side}
};
use anchor_lang::prelude::*;

pub fn calculate_liquidation_price(
    entry_price: u64,
    leverage: f64,
    side: Side
) -> Result<u64> {
    let entry_price_f64 = math::checked_float_div(entry_price as f64, crate::math::PRICE_SCALE as f64)?;
    let margin_ratio = Position::LIQUIDATION_MARGIN_BPS as f64 / 10_000.0;
    
    let max_loss_ratio = (1.0 / leverage) - margin_ratio;
    
    require!(max_loss_ratio > 0.0, PerpetualError::InvalidLeverage);
    
    let liquidation_price_f64 = match side {
        Side::Long => entry_price_f64 * (1.0 - max_loss_ratio),
        Side::Short => entry_price_f64 * (1.0 + max_loss_ratio)
    };
    
    f64_to_scaled_price(liquidation_price_f64)
}

use crate::{
    errors::TradingError,
    math::{self, f64_to_scaled_price, scaled_price_to_f64, f64_to_scaled_ratio},
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq)]
pub enum PerpSide {
    Long,  // Betting SOL price goes up
    Short, // Betting SOL price goes down
}

#[account]
pub struct PerpPosition {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub sol_custody: Pubkey,
    pub usdc_custody: Pubkey,
    
    // Position details
    pub side: PerpSide,
    pub collateral_amount: u64,    // Collateral amount in the collateral asset
    pub collateral_asset: Pubkey,  // Which asset is used as collateral (SOL or USDC custody)
    pub position_size: u64,        // SOL position size
    pub leverage: u64,             // Calculated leverage (scaled by 1e6)
    pub entry_price: u64,          // SOL price when opened (scaled by 1e6)
    pub liquidation_price: u64,    // Price at which position gets liquidated (scaled by 1e6)
    
    // Tracking
    pub open_time: i64,
    pub last_update_time: i64,
    pub unrealized_pnl: i64,       // Positive or negative P&L in USD
    
    // Risk management
    pub margin_ratio: u64,         // Current margin ratio (scaled by 1e6)
    pub is_liquidated: bool,
    
    // TP/SL Orders
    pub take_profit_price: Option<u64>,  // Take profit trigger price (scaled by 1e6)
    pub stop_loss_price: Option<u64>,    // Stop loss trigger price (scaled by 1e6)
    pub tp_sl_enabled: bool,             // Whether TP/SL is active
    
    pub bump: u8,
}


impl PerpPosition {
    pub const LEN: usize = 8 + 32 + 32 + 32 + 32 + 1 + 8 + 32 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 1 + 1 + 8 + 8 + 1 + 1 + 32; // Added TP/SL fields
    
    pub const MAX_LEVERAGE: u64 = 100 * crate::math::PRICE_SCALE; // 100.0 scaled
    pub const LIQUIDATION_THRESHOLD: u64 = 5_000; // 0.5% margin ratio triggers liquidation (scaled by 1e6)
    pub const MAINTENANCE_MARGIN: u64 = 100_000; // 10% minimum margin ratio (scaled by 1e6)
    
    pub fn update_position(&mut self, current_price: f64, current_time: i64, collateral_price: f64) -> Result<()> {
        // Convert f64 inputs to scaled format for calculations
        let _current_price_scaled = f64_to_scaled_price(current_price)?;
        let _collateral_price_scaled = f64_to_scaled_price(collateral_price)?;
        
        // Convert stored scaled values back to f64 for legacy calculations
        let entry_price_f64 = scaled_price_to_f64(self.entry_price)?;
        
        // Calculate unrealized P&L in USD
        // P&L = (price_diff / entry_price) * position_value * leverage
        let price_diff = match self.side {
            PerpSide::Long => current_price - entry_price_f64,
            PerpSide::Short => entry_price_f64 - current_price,
        };
        
        let position_value_usd = self.position_size as f64 / 1_000_000_000.0;
        
        let pnl_ratio = math::checked_float_div(price_diff, entry_price_f64)?;
        let unrealized_pnl_usd = math::checked_float_mul(pnl_ratio, position_value_usd)?;
        
        self.unrealized_pnl = (unrealized_pnl_usd * 1_000_000.0) as i64; // Store as micro-USD
        
        // Update margin ratio using scaled arithmetic where possible
        let collateral_decimals = if self.collateral_asset == self.sol_custody { 9 } else { 6 };
        let collateral_value_usd = math::checked_float_mul(
            self.collateral_amount as f64 / math::checked_powi(10.0, collateral_decimals)?,
            collateral_price
        )?;
        
        let current_equity = collateral_value_usd + unrealized_pnl_usd;
        let margin_ratio_f64 = math::checked_float_div(current_equity, position_value_usd)?;
        
        // Convert margin ratio back to scaled format for storage
        self.margin_ratio = f64_to_scaled_ratio(margin_ratio_f64)?;
        
        self.last_update_time = current_time;
        
        Ok(())
    }

    // TP/SL Management
    pub fn set_tp_sl(&mut self, take_profit: Option<f64>, stop_loss: Option<f64>) -> Result<()> {
        // Convert stored entry_price back to f64 for validation
        let entry_price_f64 = scaled_price_to_f64(self.entry_price)?;
        
        // Validate TP/SL prices
        if let Some(tp) = take_profit {
            match self.side {
                PerpSide::Long => {
                    require!(tp > entry_price_f64, TradingError::InvalidTakeProfitPrice);
                },
                PerpSide::Short => {
                    require!(tp < entry_price_f64, TradingError::InvalidTakeProfitPrice);
                }
            }
        }

        if let Some(sl) = stop_loss {
            match self.side {
                PerpSide::Long => {
                    require!(sl < entry_price_f64, TradingError::InvalidStopLossPrice);
                },
                PerpSide::Short => {
                    require!(sl > entry_price_f64, TradingError::InvalidStopLossPrice);
                }
            }
        }

        // Convert f64 inputs to scaled u64 for storage
        self.take_profit_price = if let Some(tp) = take_profit {
            Some(f64_to_scaled_price(tp)?)
        } else {
            None
        };
        
        self.stop_loss_price = if let Some(sl) = stop_loss {
            Some(f64_to_scaled_price(sl)?)
        } else {
            None
        };
        
        self.tp_sl_enabled = take_profit.is_some() || stop_loss.is_some();

        msg!("TP/SL set - TP: {:?}, SL: {:?}", take_profit, stop_loss);
        Ok(())
    }

    pub fn should_execute_tp_sl(&self, current_price: f64) -> Option<&str> {
        if !self.tp_sl_enabled {
            return None;
        }

        // Check take profit
        if let Some(tp_price_scaled) = self.take_profit_price {
            // Convert scaled TP price back to f64 for comparison
            if let Ok(tp_price_f64) = scaled_price_to_f64(tp_price_scaled) {
                let should_execute = match self.side {
                    PerpSide::Long => current_price >= tp_price_f64,
                    PerpSide::Short => current_price <= tp_price_f64,
                };
                if should_execute {
                    return Some("take_profit");
                }
            }
        }

        // Check stop loss
        if let Some(sl_price_scaled) = self.stop_loss_price {
            // Convert scaled SL price back to f64 for comparison
            if let Ok(sl_price_f64) = scaled_price_to_f64(sl_price_scaled) {
                let should_execute = match self.side {
                    PerpSide::Long => current_price <= sl_price_f64,
                    PerpSide::Short => current_price >= sl_price_f64,
                };
                if should_execute {
                    return Some("stop_loss");
                }
            }
        }

        None
    }
}
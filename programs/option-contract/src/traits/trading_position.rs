use crate::{math, state::perpetuals::Side};
use anchor_lang::prelude::*;

/// Common interface for all trading positions (perpetuals and futures)
/// Provides shared functionality while maintaining type safety
pub trait TradingPosition {
    /// Calculate current P&L based on current market price
    fn calculate_pnl(&self, current_price: u64) -> Result<i64>;
    
    /// Check if position can be liquidated at current price
    fn is_liquidatable(&self, current_price: u64) -> Result<bool>;
    
    /// Get collateral ratio (collateral / position_size)
    fn get_collateral_ratio(&self) -> Result<f64>;
    
    /// Get current leverage (position_size / collateral)
    fn get_leverage(&self) -> Result<f64>;
    
    /// Get the reference price used for P&L calculations
    fn get_reference_price(&self) -> u64;
    
    /// Get position size in USD
    fn get_size_usd(&self) -> u64;
    
    /// Get collateral value in USD
    fn get_collateral_usd(&self) -> u64;
    
    /// Get position side (Long/Short)
    fn get_side(&self) -> Side;
    
    /// Get liquidation price
    fn get_liquidation_price(&self) -> u64;
    
    /// Check if position is active and can be traded
    fn is_active(&self) -> bool;
    
    /// Update position timestamp
    fn update_timestamp(&mut self, current_time: i64);
    
    /// Calculate position health (distance from liquidation)
    /// Returns percentage: 100% = healthy, 0% = liquidation
    fn calculate_health(&self, current_price: u64) -> Result<u64> {
        let liquidation_price = self.get_liquidation_price();
        let current_price_f64 = current_price as f64;
        let liquidation_price_f64 = liquidation_price as f64;
        
        let health_percentage = match self.get_side() {
            Side::Long => {
                if current_price <= liquidation_price {
                    0 // Already liquidatable
                } else {
                    let distance = (current_price_f64 - liquidation_price_f64) / current_price_f64;
                    (distance * 100.0).min(100.0) as u64
                }
            },
            Side::Short => {
                if current_price >= liquidation_price {
                    0 // Already liquidatable
                } else {
                    let distance = (liquidation_price_f64 - current_price_f64) / liquidation_price_f64;
                    (distance * 100.0).min(100.0) as u64
                }
            }
        };
        
        Ok(health_percentage)
    }
    
    /// Calculate unrealized P&L percentage
    fn calculate_pnl_percentage(&self, current_price: u64) -> Result<f64> {
        let pnl = self.calculate_pnl(current_price)?;
        let collateral_usd = self.get_collateral_usd() as f64;
        
        if collateral_usd == 0.0 {
            return Ok(0.0);
        }
        
        Ok((pnl as f64 / collateral_usd) * 100.0)
    }
    
    /// Check if position is profitable at current price
    fn is_profitable(&self, current_price: u64) -> Result<bool> {
        let pnl = self.calculate_pnl(current_price)?;
        Ok(pnl > 0)
    }
    
    /// Calculate required margin for position size
    fn calculate_required_margin(&self, leverage: f64) -> Result<u64> {
        let size_usd = self.get_size_usd() as f64;
        let required_margin = size_usd / leverage;
        Ok(required_margin as u64)
    }
}

/// Helper functions for position calculations
pub mod position_utils {
    use super::*;
    
    /// Calculate liquidation price for a position
    pub fn calculate_liquidation_price(
        entry_price: u64,
        leverage: f64,
        side: Side,
        maintenance_margin_ratio: f64, // e.g., 0.05 for 5%
    ) -> Result<u64> {
        let entry_price_f64 = entry_price as f64;
        
        let liquidation_price = match side {
            Side::Long => {
                // Long liquidation: entry_price * (1 - (1/leverage) + maintenance_margin)
                let factor = 1.0 - (1.0 / leverage) + maintenance_margin_ratio;
                entry_price_f64 * factor
            },
            Side::Short => {
                // Short liquidation: entry_price * (1 + (1/leverage) - maintenance_margin)
                let factor = 1.0 + (1.0 / leverage) - maintenance_margin_ratio;
                entry_price_f64 * factor
            }
        };
        
        Ok(liquidation_price.max(0.0) as u64)
    }
    
    /// Calculate position value in USD
    pub fn calculate_position_value_usd(
        amount: u64,
        price: u64,
        decimals: u8,
    ) -> Result<u64> {
        let amount_normalized = amount as f64 / (10_u64.pow(decimals as u32) as f64);
        let price_normalized = price as f64 / 1_000_000.0; // Assuming 6 decimal price
        let value_usd = amount_normalized * price_normalized;
        Ok(value_usd as u64)
    }
    
    /// Calculate fees based on position size and fee rate
    pub fn calculate_trading_fee(
        size_usd: u64,
        fee_rate_bps: u64, // Fee rate in basis points (e.g., 30 = 0.3%)
    ) -> Result<u64> {
        let fee = math::checked_div(
            math::checked_mul(size_usd as u128, fee_rate_bps as u128)?,
            10_000u128
        )?;
        Ok(fee as u64)
    }
    
    /// Calculate maximum position size based on available collateral and leverage
    pub fn calculate_max_position_size(
        collateral_usd: u64,
        max_leverage: f64,
    ) -> Result<u64> {
        let max_size = (collateral_usd as f64) * max_leverage;
        Ok(max_size as u64)
    }
    
    /// Check if leverage is within acceptable bounds
    pub fn validate_leverage(
        size_usd: u64,
        collateral_usd: u64,
        max_leverage: f64,
    ) -> Result<bool> {
        if collateral_usd == 0 {
            return Ok(false);
        }
        
        let current_leverage = (size_usd as f64) / (collateral_usd as f64);
        Ok(current_leverage <= max_leverage)
    }
    
    /// Calculate break-even price including fees
    pub fn calculate_break_even_price(
        entry_price: u64,
        side: Side,
        total_fees: u64,
        position_size_usd: u64,
    ) -> Result<u64> {
        if position_size_usd == 0 {
            return Ok(entry_price);
        }
        
        let fee_per_dollar = (total_fees as f64) / (position_size_usd as f64);
        let entry_price_f64 = entry_price as f64;
        
        let break_even_price = match side {
            Side::Long => {
                // Long needs price to go up to cover fees
                entry_price_f64 * (1.0 + fee_per_dollar)
            },
            Side::Short => {
                // Short needs price to go down to cover fees
                entry_price_f64 * (1.0 - fee_per_dollar)
            }
        };
        
        Ok(break_even_price.max(0.0) as u64)
    }
}
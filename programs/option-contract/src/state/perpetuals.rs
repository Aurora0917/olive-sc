use crate::{
    errors::OptionError,
    math,
    state::{Contract, Custody, OraclePrice, Pool, User, OptionDetail},
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
    pub leverage: f64,             // Calculated leverage
    pub entry_price: f64,          // SOL price when opened
    pub liquidation_price: f64,    // Price at which position gets liquidated
    
    // Tracking
    pub open_time: i64,
    pub last_update_time: i64,
    pub unrealized_pnl: i64,       // Positive or negative P&L in USD
    
    // Risk management
    pub margin_ratio: f64,         // Current margin ratio
    pub is_liquidated: bool,
    
    pub bump: u8,
}


impl PerpPosition {
    pub const LEN: usize = 8 + 32 + 32 + 32 + 32 + 1 + 8 + 32 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 1 + 1 + 32; // Updated size
    
    pub const MAX_LEVERAGE: f64 = 100.0;
    pub const LIQUIDATION_THRESHOLD: f64 = 0.005; // 0.5% margin ratio triggers liquidation
    pub const MAINTENANCE_MARGIN: f64 = 0.10;    // 10% minimum margin ratio
    
    pub fn update_position(&mut self, current_price: f64, current_time: i64, collateral_price: f64) -> Result<()> {
        // Calculate unrealized P&L in USD
        // P&L = (price_diff / entry_price) * position_value * leverage
        let price_diff = match self.side {
            PerpSide::Long => current_price - self.entry_price,
            PerpSide::Short => self.entry_price - current_price,
        };
        
        let position_value_usd = self.position_size as f64 / 1_000_000_000.0;
        
        let pnl_ratio = math::checked_float_div(price_diff, self.entry_price)?;
        let unrealized_pnl_usd = math::checked_float_mul(pnl_ratio, position_value_usd)?;
        
        self.unrealized_pnl = (unrealized_pnl_usd * 1_000_000.0) as i64; // Store as micro-USD
        
        // Update margin ratio
        let collateral_decimals = if self.collateral_asset == self.sol_custody { 9 } else { 6 };
        let collateral_value_usd = math::checked_float_mul(
            self.collateral_amount as f64 / math::checked_powi(10.0, collateral_decimals)?,
            collateral_price
        )?;
        
        let current_equity = collateral_value_usd + unrealized_pnl_usd;
        self.margin_ratio = math::checked_float_div(current_equity, position_value_usd)?;
        
        self.last_update_time = current_time;
        
        Ok(())
    }
}
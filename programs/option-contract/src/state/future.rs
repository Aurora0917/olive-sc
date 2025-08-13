use crate::{math, state::perpetuals::Side};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Debug)]
pub enum FutureStatus {
    Pending,     // Limit order waiting for execution
    Active,      // Future is active and tradeable
    Expired,     // Future has expired, awaiting settlement
    Settled,     // Future has been settled
    Liquidated,  // Future was liquidated before expiry
}

impl Default for FutureStatus {
    fn default() -> Self {
        Self::Pending
    }
}

#[account]
#[derive(Default, Debug)]
pub struct Future {
    // Identity & References
    pub index: u64,
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,                     // Underlying asset (e.g., SOL)
    pub collateral_custody: Pubkey,          // Collateral asset (e.g., USDC)
    
    // Position Details
    pub side: Side,
    pub status: FutureStatus,
    
    // Core Future Data
    pub entry_price: u64,                    // Spot price when future was opened (scaled 6 decimals)
    pub future_price: u64,                   // Future price F = S * exp(r*T) (scaled 6 decimals)
    pub size_usd: u64,                       // Position size in USD (6 decimals)
    pub collateral_usd: u64,                 // Collateral value in USD at open (6 decimals)
    pub collateral_amount: u64,              // Actual collateral tokens deposited
    
    // Time & Expiry
    pub open_time: i64,                      // When future was opened
    pub expiry_time: i64,                    // When future expires
    pub update_time: i64,                    // Last update timestamp
    pub settlement_time: Option<i64>,        // When future was settled (None if not settled)
    
    // Fixed Interest Rate (locked at opening)
    pub fixed_interest_rate_bps: u32,        // Fixed rate in basis points (e.g., 500 = 5%)
    pub time_to_expiry_at_open: i64,         // Time to expiry in seconds when opened
    
    // Risk Management
    pub liquidation_price: u64,              // Price at which future gets liquidated
    pub maintenance_margin_bps: u64,         // Maintenance margin requirement (basis points)
    
    // Settlement Data
    pub settlement_price: Option<u64>,       // Final settlement price (None until settlement)
    pub pnl_at_settlement: Option<i64>,      // P&L at settlement (None until settlement)
    pub settlement_amount: Option<u64>,      // Amount to be claimed/paid at settlement
    
    // Fees
    pub opening_fee: u64,                    // Fee paid when opening future
    pub settlement_fee: u64,                 // Fee to be paid at settlement
    
    // Asset Amounts (For settlement)
    pub locked_amount: u64,                  // Amount locked in pool for this future
    
    // Limit Order Fields (for pending futures)
    pub trigger_price: Option<u64>,          // Price that triggers execution (None for market orders)
    pub trigger_above_threshold: bool,       // true = execute when price >= trigger, false = when price <= trigger
    pub max_slippage: u64,                   // Maximum acceptable slippage in basis points
    pub execution_time: Option<i64>,         // When limit order was executed (None if pending)
    
    // Metadata
    pub bump: u8,
}

impl Future {
    pub const LEN: usize = 8 + std::mem::size_of::<Future>() + 16; // Extra padding for Option fields
    
    // Maximum leverage for futures (lower than perps due to expiry risk)
    pub const MAX_LEVERAGE: f64 = 250.0;
    pub const MIN_INITIAL_MARGIN_BPS: u64 = 40;  // 1% initial margin for 100x
    pub const MAINTENANCE_MARGIN_BPS: u64 = 20;   // 0.5% maintenance margin
    pub const OPENING_FEE_BPS: u64 = 10;           // 0.1% opening fee
    pub const SETTLEMENT_FEE_BPS: u64 = 5;         // 0.05% settlement fee
    
    /// Calculate theoretical future price using F = S * exp(r * T)
    pub fn calculate_theoretical_price(
        spot_price: f64,
        fixed_rate_bps: u32,
        time_to_expiry_years: f64,
    ) -> Result<f64> {
        let rate = (fixed_rate_bps as f64) / 10_000.0;
        let theoretical_price = spot_price * (rate * time_to_expiry_years).exp();
        Ok(theoretical_price)
    }
    
    /// Get current leverage of the future position
    pub fn get_current_leverage(&self) -> Result<f64> {
        if self.collateral_usd == 0 {
            return Ok(0.0);
        }
        Ok((self.size_usd as f64) / (self.collateral_usd as f64))
    }
    
    /// Calculate P&L based on current spot price vs future price
    /// PNL (long) = (size/P_e) * (P_m * exp(r*t_1) - P_e * exp(r*t_0))
    /// where P_e = entry price, P_m = current mark price, 
    /// t_0 = time from open to expiry, t_1 = time from now to expiry
    pub fn calculate_pnl(&self, current_spot_price: u64, current_time: i64) -> Result<i64> {
        // Calculate time factors
        let t_0 = self.time_to_expiry_at_open as f64 / (365.25 * 24.0 * 3600.0); // Original time to expiry in years
        let time_elapsed = (current_time - self.open_time) as f64;
        let t_1 = ((self.time_to_expiry_at_open as f64) - time_elapsed) / (365.25 * 24.0 * 3600.0); // Remaining time in years
        let t_1 = t_1.max(0.0); // Cannot be negative
        
        // Interest rate
        let r = (self.fixed_interest_rate_bps as f64) / 10_000.0;
        
        // Calculate exp factors
        let exp_rt_0 = (r * t_0).exp();
        let exp_rt_1 = (r * t_1).exp();
        
        // Convert prices to f64 for calculation
        let p_e = (self.entry_price as f64) / 1_000_000.0;
        let p_m = (current_spot_price as f64) / 1_000_000.0;
        let size = (self.size_usd as f64) / 1_000_000.0;
        
        // Calculate PNL based on side
        let pnl_usd = match self.side {
            Side::Long => {
                // PNL (long) = (size/P_e) * (P_m * exp(r*t_1) - P_e * exp(r*t_0))
                let quantity = size / p_e;
                quantity * (p_m * exp_rt_1 - p_e * exp_rt_0)
            },
            Side::Short => {
                // PNL (short) = -(size/P_e) * (P_m * exp(r*t_1) - P_e * exp(r*t_0))
                let quantity = size / p_e;
                -quantity * (p_m * exp_rt_1 - p_e * exp_rt_0)
            }
        };
        
        // Convert back to scaled integer
        Ok((pnl_usd * 1_000_000.0) as i64)
    }
    
    /// Check if future should be liquidated based on maintenance margin
    pub fn is_liquidatable(&self, current_spot_price: u64, current_time: i64) -> Result<bool> {
        if self.status != FutureStatus::Active {
            return Ok(false);
        }
        
        let pnl = self.calculate_pnl(current_spot_price, current_time)?;
        
        // Calculate current equity (collateral + unrealized PnL)
        let current_equity = if pnl >= 0 {
            self.collateral_usd + (pnl as u64)
        } else {
            let loss = (-pnl) as u64;
            if loss >= self.collateral_usd {
                0 // Insolvent
            } else {
                self.collateral_usd - loss
            }
        };
        
        // Calculate maintenance margin requirement
        let maintenance_margin_required = math::checked_div(
            math::checked_mul(self.size_usd as u128, self.maintenance_margin_bps as u128)?,
            10_000u128,
        )? as u64;
        
        Ok(current_equity < maintenance_margin_required)
    }
    
    /// Calculate liquidation price using the formula:
    /// P_liq = P_e * exp(r*t_0)/exp(r*t_1) - ((collateral - close_fee - (size/max_lev)) * P_e * exp(r*t_0))/(size * exp(r*t_1))
    pub fn calculate_liquidation_price(&self, current_time: i64) -> Result<u64> {
        // Calculate time factors
        let t_0 = self.time_to_expiry_at_open as f64 / (365.25 * 24.0 * 3600.0); // Original time to expiry in years
        let time_elapsed = (current_time - self.open_time) as f64;
        let t_1 = ((self.time_to_expiry_at_open as f64) - time_elapsed) / (365.25 * 24.0 * 3600.0); // Remaining time in years
        let t_1 = t_1.max(0.001); // Avoid division by zero near expiry
        
        // Interest rate
        let r = (self.fixed_interest_rate_bps as f64) / 10_000.0;
        
        // Calculate exp factors
        let exp_rt_0 = (r * t_0).exp();
        let exp_rt_1 = (r * t_1).exp();
        let exp_ratio = exp_rt_0 / exp_rt_1;
        
        // Convert to f64 for calculation
        let p_e = (self.entry_price as f64) / 1_000_000.0;
        let size = (self.size_usd as f64) / 1_000_000.0;
        let collateral = (self.collateral_usd as f64) / 1_000_000.0;
        
        // Calculate close fee (settlement fee)
        let close_fee = size * (Self::SETTLEMENT_FEE_BPS as f64) / 10_000.0;
        
        // Get max leverage to calculate minimum margin
        let max_leverage = Self::MAX_LEVERAGE;
        let min_margin = size / max_leverage;
        
        // Calculate liquidation price based on side
        let p_liq = match self.side {
            Side::Long => {
                // For long: liquidation happens when price drops
                // P_liq = P_e * exp_ratio - ((collateral - close_fee - min_margin) * P_e * exp_ratio) / size
                let numerator = (collateral - close_fee - min_margin) * p_e * exp_ratio;
                p_e * exp_ratio - (numerator / size)
            },
            Side::Short => {
                // For short: liquidation happens when price rises
                // P_liq = P_e * exp_ratio + ((collateral - close_fee - min_margin) * P_e * exp_ratio) / size
                let numerator = (collateral - close_fee - min_margin) * p_e * exp_ratio;
                p_e * exp_ratio + (numerator / size)
            }
        };
        
        // Convert back to scaled integer (ensure positive)
        let p_liq_scaled = (p_liq * 1_000_000.0).max(0.0) as u64;
        Ok(p_liq_scaled)
    }
    
    /// Check if future has expired
    pub fn is_expired(&self, current_time: i64) -> bool {
        current_time >= self.expiry_time
    }
    
    /// Get time remaining until expiry in seconds
    pub fn time_to_expiry(&self, current_time: i64) -> i64 {
        (self.expiry_time - current_time).max(0)
    }
    
    /// Calculate settlement amount based on final settlement price
    pub fn calculate_settlement_amount(&self, settlement_price: u64, settlement_time: i64) -> Result<(u64, i64)> {
        // Calculate P&L at settlement (at expiry, t_1 = 0)
        let settlement_pnl = self.calculate_pnl(settlement_price, settlement_time)?;
        
        // Calculate net settlement (collateral + PnL - fees)
        let net_settlement = (self.collateral_usd as i64) + settlement_pnl - (self.settlement_fee as i64);
        
        // Settlement amount is non-negative
        let settlement_amount = if net_settlement > 0 {
            net_settlement as u64
        } else {
            0
        };
        
        Ok((settlement_amount, settlement_pnl))
    }
    
    /// Update future status and settlement data
    pub fn settle_future(
        &mut self,
        settlement_price: u64,
        current_time: i64,
    ) -> Result<u64> {
        require!(
            self.status == FutureStatus::Expired,
            crate::errors::FutureError::FutureNotExpired
        );
        
        let (settlement_amount, pnl) = self.calculate_settlement_amount(settlement_price, current_time)?;
        
        self.status = FutureStatus::Settled;
        self.settlement_time = Some(current_time);
        self.settlement_price = Some(settlement_price);
        self.pnl_at_settlement = Some(pnl);
        self.settlement_amount = Some(settlement_amount);
        self.update_time = current_time;
        
        Ok(settlement_amount)
    }
    
    /// Mark future as expired
    pub fn mark_expired(&mut self, current_time: i64) -> Result<()> {
        require!(
            self.is_expired(current_time),
            crate::errors::FutureError::FutureNotYetExpired
        );
        require!(
            self.status == FutureStatus::Active,
            crate::errors::FutureError::FutureNotActive
        );
        
        self.status = FutureStatus::Expired;
        self.update_time = current_time;
        
        Ok(())
    }
    
    /// Mark future as liquidated
    pub fn liquidate_future(
        &mut self,
        liquidation_price: u64,
        current_time: i64,
    ) -> Result<(u64, i64)> {
        require!(
            self.status == FutureStatus::Active,
            crate::errors::FutureError::FutureNotActive
        );
        
        let (remaining_collateral, pnl) = self.calculate_settlement_amount(liquidation_price, current_time)?;
        
        self.status = FutureStatus::Liquidated;
        self.settlement_time = Some(current_time);
        self.settlement_price = Some(liquidation_price);
        self.pnl_at_settlement = Some(pnl);
        self.settlement_amount = Some(remaining_collateral);
        self.update_time = current_time;
        
        Ok((remaining_collateral, pnl))
    }
    
    /// Update future with new data (for partial closes, etc.)
    pub fn update_future(
        &mut self,
        new_size_usd: Option<u64>,
        new_collateral_usd: Option<u64>,
        new_collateral_amount: Option<u64>,
        current_time: i64,
    ) -> Result<()> {
        require!(
            self.status == FutureStatus::Active,
            crate::errors::FutureError::FutureNotActive
        );
        
        if let Some(size) = new_size_usd {
            self.size_usd = size;
        }
        
        if let Some(collateral_usd) = new_collateral_usd {
            self.collateral_usd = collateral_usd;
        }
        
        if let Some(collateral_amount) = new_collateral_amount {
            self.collateral_amount = collateral_amount;
        }
        
        self.update_time = current_time;
        Ok(())
    }
}

// Note: Future does not implement TradingPosition trait because it requires
// time-dependent calculations that don't fit the trait's interface.
// Future has its own methods: calculate_pnl(price, time) and is_liquidatable(price, time)
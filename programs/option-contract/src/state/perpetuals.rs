use crate::{
    math::{self},
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Debug)]
pub enum Side {
    Long,
    Short,
}

impl Default for Side {
    fn default() -> Self {
        Self::Long
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Debug)]
pub enum OrderType {
    Market,     // Market position (immediate execution)
    Limit,      // Limit order (pending execution)
}

impl Default for OrderType {
    fn default() -> Self {
        Self::Market
    }
}

#[account]
#[derive(Default, Debug)]
pub struct Position {
    // Identity & References
    pub index: u64,
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,                     // Position asset (e.g., SOL)
    pub collateral_custody: Pubkey,          // Collateral asset (e.g., USDC)
    
    // Position Type & Status
    pub order_type: OrderType,         // Market or Limit
    pub side: Side,
    pub is_liquidated: bool,
    
    // Core Position Data
    pub price: u64,                          // Entry price (scaled) - for limit: trigger price
    pub size_usd: u64,                       // Position size in USD
    pub collateral_usd: u64,                // Collateral value in USD at open
    pub open_time: i64,                     // When position was created
    pub update_time: i64,                   // Track updates
    pub execution_time: Option<i64>,        // When limit order was executed (None for market orders)
    
    // Risk Management (Set at open, used for liquidation)
    pub liquidation_price: u64,              // Pre-calculated for efficiency
    
    // Borrow Fee Tracking (side-specific)
    pub cumulative_interest_snapshot: u128,  // Pool's cumulative borrow rate at position open (side-specific)
    pub last_borrow_fees_update_time: i64,   // When borrow fees were last calculated/updated
    
    // Accrued Amounts (settled on close)
    pub accrued_borrow_fees: u64,           // Accrued borrow fees (always positive, always paid by position)
    
    // Fee Tracking
    pub borrow_fees_paid: u64,               // All fees paid
    pub trade_fees: u64,                     // Exiting fee
    
    // Asset Amounts (For settlement)
    pub locked_amount: u64,                  // Locked in pool
    pub collateral_amount: u64,             // Actual collateral tokens
    
    // TP/SL Orderbook reference (optional advanced feature)
    pub tp_sl_orderbook: Option<Pubkey>,    // Optional reference to TpSlOrderbook account
    
    // Limit Order (for limit perp)
    pub trigger_price: Option<u64>,         // Price to execute limit order
    pub trigger_above_threshold: bool,      // true = execute when price >= trigger
    
    pub bump: u8,
}


impl Position {
    pub const LEN: usize = 8 + std::mem::size_of::<Position>() + 33; // Added 33 bytes for Option<Pubkey>
    
    // 250x leverage = 0.4% initial margin
    pub const MAX_LEVERAGE: f64 = 250.0;
    pub const MIN_INITIAL_MARGIN_BPS: u64 = 40; // 1.0% for 100x leverage
    pub const LIQUIDATION_MARGIN_BPS: u64 = 20; // 0.4% liquidation threshold
    pub const EXITING_FEE_BPS: u64 = 10;
    
    pub fn get_initial_leverage(&self) -> Result<u64> {
        if self.collateral_usd == 0 {
            return Ok(0);
        }
        math::checked_as_u64(math::checked_div(
            self.size_usd as u128,
            self.collateral_usd as u128,
        )?)
    }
    
    pub fn update_position(
        &mut self,
        new_size_usd: Option<u64>,
        new_collateral_usd: Option<u64>,
        new_collateral_amount: Option<u64>,
        current_time: i64,
    ) -> Result<()> {
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
    
    pub fn update_accrued_borrow_fees(
        &mut self,
        borrow_fee_payment: u64,
        new_interest_snapshot: u128,
        current_time: i64,
    ) -> Result<()> {
        self.accrued_borrow_fees = math::checked_add(self.accrued_borrow_fees, borrow_fee_payment)?;
        self.cumulative_interest_snapshot = new_interest_snapshot;
        self.last_borrow_fees_update_time = current_time;
        self.update_time = current_time;
        Ok(())
    }

    // Calculate and accrue time-based borrow fees
    pub fn calculate_and_accrue_borrow_fees(
        &mut self,
        current_time: i64,
        current_borrow_rate_bps: u32, // Annual percentage rate in basis points
    ) -> Result<u64> {
        // Limit orders don't accrue borrow fees
        if self.order_type == OrderType::Limit {
            return Ok(0);
        }
        
        if current_time <= self.last_borrow_fees_update_time {
            return Ok(0); // No time elapsed
        }

        let time_elapsed_seconds = math::checked_sub(current_time, self.last_borrow_fees_update_time)? as u128;
        
        // Convert APR to per-second rate: rate_bps / (365 * 24 * 3600 * 10000)
        // Formula: (position_size_usd * rate_bps * time_elapsed_seconds) / (365 * 24 * 3600 * 10000)
        let seconds_per_year = 365u128 * 24 * 3600; // 31,536,000 seconds per year
        let basis_points_scale = 10_000u128;
        
        let borrow_fee_accrued = math::checked_div(
            math::checked_mul(
                math::checked_mul(self.size_usd as u128, current_borrow_rate_bps as u128)?,
                time_elapsed_seconds
            )?,
            math::checked_mul(seconds_per_year, basis_points_scale)?
        )?;

        let borrow_fee_accrued_u64 = math::checked_as_u64(borrow_fee_accrued)?;

        // Update accrued fees and timestamp
        self.accrued_borrow_fees = math::checked_add(self.accrued_borrow_fees, borrow_fee_accrued_u64)?;
        self.last_borrow_fees_update_time = current_time;
        self.update_time = current_time;

        Ok(borrow_fee_accrued_u64)
    }

    pub fn should_execute_limit_order(&self, current_price: u64) -> bool {
        if self.order_type != OrderType::Limit {
            return false;
        }
        
        if let Some(trigger_price) = self.trigger_price {
            if self.trigger_above_threshold {
                current_price >= trigger_price
            } else {
                current_price <= trigger_price
            }
        } else {
            false
        }
    }
    
    pub fn execute_limit_order(&mut self, execution_price: u64, current_time: i64) -> Result<()> {
        self.order_type = OrderType::Market;
        self.price = execution_price;
        self.trigger_price = None;
        self.execution_time = Some(current_time);  // Track when limit order was executed
        self.update_time = current_time;
        Ok(())
    }
    
    /// Check if this position was originally a limit order
    pub fn was_limit_order(&self) -> bool {
        // If execution_time exists and is different from open_time, it was a limit order
        if let Some(execution_time) = self.execution_time {
            execution_time != self.open_time
        } else {
            // If execution_time is None, it's still a pending limit order
            self.order_type == OrderType::Limit
        }
    }
    
    /// Check if this is a pending limit order
    pub fn is_pending_limit_order(&self) -> bool {
        self.order_type == OrderType::Limit && self.execution_time.is_none()
    }
    
    /// Check if this is an executed limit order (now market position)
    pub fn is_executed_limit_order(&self) -> bool {
        self.order_type == OrderType::Market && 
        self.execution_time.is_some() && 
        self.execution_time.unwrap() != self.open_time
    }

    pub fn is_liquidatable(&self, current_price: u64) -> bool {
        if self.order_type == OrderType::Limit {
            return false; // Can't liquidate limit orders
        }
        
        match self.side {
            Side::Long => current_price <= self.liquidation_price,
            Side::Short => current_price >= self.liquidation_price,
        }
    }
    
    pub fn is_liquidatable_by_margin(&self, current_price: u64) -> Result<bool> {
        if self.order_type == OrderType::Limit {
            return Ok(false);
        }
        
        let pnl = self.calculate_pnl(current_price)?;
        let current_equity = if pnl >= 0 {
            self.collateral_usd + pnl as u64
        } else {
            let loss = (-pnl) as u64;
            if loss >= self.collateral_usd {
                0
            } else {
                self.collateral_usd - loss
            }
        };
        
        let margin_ratio_bps = math::checked_as_u64(math::checked_div(
            math::checked_mul(current_equity as u128, 10_000u128)?,
            self.size_usd as u128,
        )?)?;
        
        Ok(margin_ratio_bps <= Self::LIQUIDATION_MARGIN_BPS)
    }
    
    pub fn calculate_pnl(&self, current_price: u64) -> Result<i64> {
        let price_diff = match self.side {
            Side::Long => current_price as i64 - self.price as i64,
            Side::Short => self.price as i64 - current_price as i64,
        };
        
        let pnl = math::checked_div(
            math::checked_mul(price_diff as i128, self.size_usd as i128)?,
            self.price as i128,
        )?;
        
        Ok(pnl as i64)
    }    
}
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
pub enum PositionType {
    Market,     // Market position (immediate execution)
    Limit,      // Limit order (pending execution)
}

impl Default for PositionType {
    fn default() -> Self {
        Self::Market
    }
}

#[account]
#[derive(Default, Debug)]
pub struct Position {
    // Identity & References
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,                     // Position asset (e.g., SOL)
    pub collateral_custody: Pubkey,          // Collateral asset (e.g., USDC)
    
    // Position Type & Status
    pub position_type: PositionType,         // Market or Limit
    pub side: Side,
    pub is_liquidated: bool,
    
    // Core Position Data
    pub price: u64,                          // Entry price (scaled) - for limit: trigger price
    pub size_usd: u64,                       // Position size in USD
    pub borrow_size_usd: u64,               // Borrowed amount in USD
    pub collateral_usd: u64,                // Collateral value in USD at open
    pub open_time: i64,
    pub update_time: i64,                   // Track updates
    
    // Risk Management (Set at open, used for liquidation)
    pub liquidation_price: u64,              // Pre-calculated for efficiency
    pub initial_margin_bps: u64,            // e.g., 400 = 4% for 250x leverage
    pub maintenance_margin_bps: u64,        // e.g., 200 = 2% for liquidation
    
    // Funding & Interest Tracking
    pub cumulative_interest_snapshot: u128,  // Pool's cumulative at position open
    pub cumulative_funding_snapshot: u128,   // Pool's funding at position open
    
    // Fee Tracking
    pub total_fees_paid: u64,               // All fees paid
    pub opening_fee_paid: u64,              // Opening fee
    
    // Asset Amounts (For settlement)
    pub locked_amount: u64,                  // Locked in pool
    pub collateral_amount: u64,             // Actual collateral tokens
    
    // TP/SL Storage (Store on-chain, backend checks & executes)
    pub take_profit_price: Option<u64>,     // Backend monitors, executes when hit
    pub stop_loss_price: Option<u64>,       // Backend monitors, executes when hit
    
    // Limit Order (for limit perp)
    pub trigger_price: Option<u64>,         // Price to execute limit order
    pub trigger_above_threshold: bool,      // true = execute when price >= trigger
    
    pub bump: u8,
}


impl Position {
    pub const LEN: usize = 8 + std::mem::size_of::<Position>();
    
    // 250x leverage = 0.4% initial margin
    pub const MAX_LEVERAGE: u64 = 250;
    pub const MIN_INITIAL_MARGIN_BPS: u64 = 40; // 0.4% for 250x leverage
    pub const LIQUIDATION_MARGIN_BPS: u64 = 20; // 0.2% liquidation threshold
    
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
    
    pub fn update_tp_sl(
        &mut self,
        take_profit: Option<u64>,
        stop_loss: Option<u64>,
    ) -> Result<()> {
        self.take_profit_price = take_profit;
        self.stop_loss_price = stop_loss;
        Ok(())
    }

    pub fn should_execute_limit_order(&self, current_price: u64) -> bool {
        if self.position_type != PositionType::Limit {
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
        self.position_type = PositionType::Market;
        self.price = execution_price;
        self.trigger_price = None;
        self.open_time = current_time;
        self.update_time = current_time;
        Ok(())
    }

    pub fn is_liquidatable(&self, current_price: u64) -> bool {
        if self.position_type == PositionType::Limit {
            return false; // Can't liquidate limit orders
        }
        
        match self.side {
            Side::Long => current_price <= self.liquidation_price,
            Side::Short => current_price >= self.liquidation_price,
        }
    }
    
    pub fn is_liquidatable_by_margin(&self, current_price: u64) -> Result<bool> {
        if self.position_type == PositionType::Limit {
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
        
        Ok(margin_ratio_bps <= self.maintenance_margin_bps)
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
    
    pub fn calculate_funding_payment(&self, current_cumulative_funding: u128) -> Result<i64> {
        let funding_diff = current_cumulative_funding - self.cumulative_funding_snapshot;
        let funding_payment = math::checked_mul(
            funding_diff as i128,
            self.size_usd as i128,
        )?;
        
        let final_payment = match self.side {
            Side::Long => funding_payment,
            Side::Short => -funding_payment,
        };
        
        Ok(final_payment as i64)
    }
    
    pub fn calculate_interest_payment(&self, current_cumulative_interest: u128) -> Result<u64> {
        let interest_diff = current_cumulative_interest - self.cumulative_interest_snapshot;
        math::checked_as_u64(math::checked_mul(
            interest_diff as u128,
            self.borrow_size_usd as u128,
        )?)
    }
}
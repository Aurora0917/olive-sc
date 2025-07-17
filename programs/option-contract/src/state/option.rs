use anchor_lang::prelude::*;
use crate::{utils::option_pricing::*, math::{self, scaled_price_to_f64}};

#[account]
pub struct OptionDetail {
    pub index: u64,
    pub owner: Pubkey,
    pub amount: u64,
    pub quantity: u64,
    pub strike_price: u64,        // Strike price scaled by 1e6 (6 decimals)
    pub period: u64,
    pub expired_date: i64,
    pub purchase_date: u64,
    pub option_type: u8, // 0: call, 1: put

    pub premium: u64,
    pub premium_asset: Pubkey, // pay_custody key
    pub profit: u64,
    pub locked_asset: Pubkey, // locked custody key

    pub pool: Pubkey,
    pub custody: Pubkey,

    pub exercised: u64,
    pub bought_back: u64, // time Stamp when
    pub claimed: u64,     // claimable amount after automatically exercise by bot.
    pub valid: bool,      // false - invalid/expired/exercised, true - valid
    pub bump: u8,
    pub limit_price: u64,
    pub executed: bool,
    
    // NEW FIELD
    pub entry_price: u64,     // Underlying asset price when option was purchased (scaled by 1e6)
    pub last_update_time: i64, // Last time option was updated
    
    // TP/SL FIELDS
    pub take_profit_price: Option<u64>,  // Take profit price (scaled by 1e6)
    pub stop_loss_price: Option<u64>,    // Stop loss price (scaled by 1e6)
    
    // TP/SL Orderbook reference (optional advanced feature)
    pub tp_sl_orderbook: Option<Pubkey>, // Optional reference to TpSlOrderbook account
}

impl OptionDetail {
    // Updated length calculation: added 8 bytes for entry_price (u64) + 8 bytes for last_update_time (i64) + 18 bytes for TP/SL (Option<u64> * 2) + 33 bytes for Option<Pubkey>
    pub const LEN: usize = 8 * 15 + 4 + 32 * 5 + 8 + 18 + 33;

    /// Update option with current market data (similar to update_position)
    pub fn update_option(
        &mut self, 
        current_price: f64, 
        current_time: i64,
        token_locked: u64,
        token_owned: u64,
        is_sol: bool
    ) -> Result<()> {
        // Check if option is still valid
        if !self.valid || current_time > self.expired_date {
            return Ok(()); // Don't update invalid/expired options
        }

        // Calculate time to expiration in years
        let time_to_expiry = math::checked_float_div(
            (self.expired_date - current_time) as f64,
            365.25 * 24.0 * 3600.0 // seconds in a year
        )?;

        // If time to expiry is <= 0, option is expired
        if time_to_expiry <= 0.0 {
            self.valid = false;
            return Ok(());
        }

        // Calculate current option value using Black-Scholes with dynamic rates
        // Convert scaled strike price back to f64 for calculation
        let strike_price_f64 = scaled_price_to_f64(self.strike_price)?;
        let current_option_value = black_scholes_with_borrow_rate(
            current_price,
            strike_price_f64,
            time_to_expiry,
            self.option_type == 0, // 0 = call, 1 = put
            token_locked,
            token_owned,
            is_sol
        )?;

        // Calculate profit/loss using proper decimal math
        // Use the same scaling as the premium asset to avoid precision loss
        let current_value_scaled = math::checked_decimal_mul(
            math::checked_as_u64(current_option_value * 1_000_000.0)?, // Convert to micro units
            -6, // micro decimals
            1,
            0,
            -6, // target 6 decimals to match common stablecoin precision
        )?;
        
        if current_value_scaled > self.premium {
            self.profit = current_value_scaled - self.premium;
        } else {
            self.profit = 0; // No negative profit, just loss
        }

        // Check if limit price conditions are met for automatic execution
        if !self.executed && self.limit_price > 0 {
            let should_execute = if self.option_type == 0 { // Call option
                current_price >= (self.limit_price as f64 / 1_000_000.0) // Assuming 6 decimal scaling
            } else { // Put option
                current_price <= (self.limit_price as f64 / 1_000_000.0)
            };

            if should_execute {
                self.executed = true;
                self.exercised = current_time as u64;
            }
        }

        // Check TP/SL conditions for automatic execution
        if self.valid && !self.executed {
            let mut should_execute = false;
            
            // Check take profit
            if let Some(tp_price) = self.take_profit_price {
                let tp_price_f64 = tp_price as f64 / 1_000_000.0;
                if self.option_type == 0 { // Call option
                    // For calls, take profit when underlying price goes above TP
                    if current_price >= tp_price_f64 {
                        should_execute = true;
                    }
                } else { // Put option
                    // For puts, take profit when underlying price goes below TP
                    if current_price <= tp_price_f64 {
                        should_execute = true;
                    }
                }
            }
            
            // Check stop loss
            if let Some(sl_price) = self.stop_loss_price {
                let sl_price_f64 = sl_price as f64 / 1_000_000.0;
                if self.option_type == 0 { // Call option
                    // For calls, stop loss when underlying price goes below SL
                    if current_price <= sl_price_f64 {
                        should_execute = true;
                    }
                } else { // Put option
                    // For puts, stop loss when underlying price goes above SL
                    if current_price >= sl_price_f64 {
                        should_execute = true;
                    }
                }
            }
            
            if should_execute {
                self.executed = true;
                self.exercised = current_time as u64;
            }
        }

        // Update last update time
        self.last_update_time = current_time;

        Ok(())
    }
}
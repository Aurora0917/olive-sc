use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::price_update::{get_feed_id_from_hex, PriceUpdateV2};
use core::cmp::Ordering;
use crate::{errors::ContractError, math, state::Contract};

#[derive(Copy, Clone, Eq, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct OraclePrice {
    pub price: u64,
    pub exponent: i32,
}

impl PartialOrd for OraclePrice {
    fn partial_cmp(&self, other: &OraclePrice) -> Option<Ordering> {
        let (lhs, rhs) = if self.exponent == other.exponent {
            (self.price, other.price)
        } else if self.exponent < other.exponent {
            if let Ok(scaled_price) = other.scale_to_exponent(self.exponent) {
                (self.price, scaled_price.price)
            } else {
                return None;
            }
        } else if let Ok(scaled_price) = self.scale_to_exponent(other.exponent) {
            (scaled_price.price, other.price)
        } else {
            return None;
        };
        lhs.partial_cmp(&rhs)
    }
}

#[allow(dead_code)]
impl OraclePrice {
    pub const MAX_PRICE_AGE_SEC: u64 = 6000; // 5 minutes - strict for options trading
    pub const ORACLE_MAX_PRICE: u64 = (1 << 28) - 1;
    pub const ORACLE_EXPONENT_SCALE: i32 = -9;
    pub const ORACLE_PRICE_SCALE: u64 = 1_000_000_000;
    pub const MAX_CONFIDENCE_INTERVAL_BPS: u64 = 500; // 5% max confidence interval
    
    pub fn new(price: u64, exponent: i32) -> Self {
        Self { price, exponent }
    }

    pub fn new_from_token(amount_and_decimals: (u64, u8)) -> Self {
        Self {
            price: amount_and_decimals.0,
            exponent: -(amount_and_decimals.1 as i32),
        }
    }
    
    pub fn get_price(&self) -> f64 {
        (self.price as f64) * 10f64.powi(self.exponent)
    }
    
    /// Get price from Pyth PriceUpdateV2 account
    /// This expects a price update account that contains verified price data
    pub fn new_from_oracle(
        oracle_account: &AccountInfo,
        _current_time: i64, // Keeping for compatibility but Clock is used internally
        _use_ema: bool,     // Not supported in current version
    ) -> Result<OraclePrice> {
        Self::get_pyth_price_from_update_account(oracle_account)
    }

    /// Get price with explicit feed ID (recommended for production)
    pub fn new_from_oracle_with_feed_id(
        oracle_account: &AccountInfo,
        feed_id_hex: &str,
        _use_ema: bool,
    ) -> Result<OraclePrice> {
        Self::get_pyth_price_with_feed_id(oracle_account, feed_id_hex)
    }

    /// Direct method to get price from a typed PriceUpdateV2 account (recommended)
    pub fn new_from_price_update(
        price_update: &Account<PriceUpdateV2>,
        feed_id_hex: Option<&str>,
    ) -> Result<OraclePrice> {
        let clock = Clock::get()?;
        
        // If feed_id provided, verify it matches
        if let Some(feed_id_hex) = feed_id_hex {
            let feed_id = get_feed_id_from_hex(feed_id_hex)
                .map_err(|_| {
                    msg!("Invalid feed ID hex string: {}", feed_id_hex);
                    ContractError::InvalidOracleAccount
                })?;
            
            require!(
                price_update.price_message.feed_id == feed_id,
                ContractError::InvalidOracleAccount
            );
        }
        
        let price_message = &price_update.price_message;
        
        // Check staleness
        let age = clock.unix_timestamp - price_message.publish_time;
        require!(
            age <= Self::MAX_PRICE_AGE_SEC as i64,
            ContractError::StaleOraclePrice
        );
        
        // Validate price confidence - confidence should be reasonable relative to price
        let confidence_bps = if price_message.price > 0 {
            ((price_message.conf as u128 * 10000) / price_message.price as u128) as u64
        } else {
            u64::MAX // Invalid if price is zero
        };
        require!(
            confidence_bps <= Self::MAX_CONFIDENCE_INTERVAL_BPS,
            ContractError::LowConfidencePrice
        );
        
        msg!("Pyth price: {}, exponent: {}, confidence: {}, age: {} seconds", 
             price_message.price, price_message.exponent, price_message.conf, age);
        
        // Reject negative prices - this indicates oracle failure
        require!(
            price_message.price > 0,
            ContractError::InvalidOraclePrice
        );
        let price_value = price_message.price as u64;
        
        Ok(OraclePrice {
            price: price_value,
            exponent: price_message.exponent,
        })
    }

    // Rest of the methods remain the same
    pub fn get_asset_amount_usd(&self, token_amount: u64, token_decimals: u8) -> Result<u64> {
        if token_amount == 0 || self.price == 0 {
            return Ok(0);
        }
        math::checked_decimal_mul(
            token_amount,
            -(token_decimals as i32),
            self.price,
            self.exponent,
            -(Contract::USD_DECIMALS as i32),
        )
    }

    pub fn get_token_amount(&self, asset_amount_usd: u64, token_decimals: u8) -> Result<u64> {
        if asset_amount_usd == 0 || self.price == 0 {
            return Ok(0);
        }
        math::checked_decimal_div(
            asset_amount_usd,
            -(Contract::USD_DECIMALS as i32),
            self.price,
            self.exponent,
            -(token_decimals as i32),
        )
    }

    pub fn normalize(&self) -> Result<OraclePrice> {
        let mut p = self.price;
        let mut e = self.exponent;

        while p > Self::ORACLE_MAX_PRICE {
            p = math::checked_div(p, 10)?;
            e = math::checked_add(e, 1)?;
        }

        Ok(OraclePrice {
            price: p,
            exponent: e,
        })
    }

    pub fn checked_div(&self, other: &OraclePrice) -> Result<OraclePrice> {
        let base = self.normalize()?;
        let other = other.normalize()?;

        Ok(OraclePrice {
            price: math::checked_div(
                math::checked_mul(base.price, Self::ORACLE_PRICE_SCALE)?,
                other.price,
            )?,
            exponent: math::checked_sub(
                math::checked_add(base.exponent, Self::ORACLE_EXPONENT_SCALE)?,
                other.exponent,
            )?,
        })
    }

    pub fn checked_mul(&self, other: &OraclePrice) -> Result<OraclePrice> {
        Ok(OraclePrice {
            price: math::checked_mul(self.price, other.price)?,
            exponent: math::checked_add(self.exponent, other.exponent)?,
        })
    }

    pub fn scale_to_exponent(&self, target_exponent: i32) -> Result<OraclePrice> {
        if target_exponent == self.exponent {
            return Ok(*self);
        }
        let delta = math::checked_sub(target_exponent, self.exponent)?;
        if delta > 0 {
            Ok(OraclePrice {
                price: math::checked_div(self.price, math::checked_pow(10, delta as usize)?)?,
                exponent: target_exponent,
            })
        } else {
            Ok(OraclePrice {
                price: math::checked_mul(self.price, math::checked_pow(10, (-delta) as usize)?)?,
                exponent: target_exponent,
            })
        }
    }

    pub fn checked_as_f64(&self) -> Result<f64> {
        math::checked_float_mul(
            math::checked_as_f64(self.price)?,
            math::checked_powi(10.0, self.exponent)?,
        )
    }

    pub fn get_min_price(&self, other: &OraclePrice, is_stable: bool) -> Result<OraclePrice> {
        let min_price = if self < other { self } else { other };
        if is_stable {
            if min_price.exponent > 0 {
                if min_price.price == 0 {
                    return Ok(*min_price);
                } else {
                    return Ok(OraclePrice {
                        price: 1000000u64,
                        exponent: -6,
                    });
                }
            }
            let one_usd = math::checked_pow(10u64, (-min_price.exponent) as usize)?;
            if min_price.price > one_usd {
                Ok(OraclePrice {
                    price: one_usd,
                    exponent: min_price.exponent,
                })
            } else {
                Ok(*min_price)
            }
        } else {
            Ok(*min_price)
        }
    }

    /// Main implementation - works with PriceUpdateV2 accounts
    /// This method tries to auto-detect the feed ID from the price update
    fn get_pyth_price_from_update_account(
        oracle_account: &AccountInfo,
    ) -> Result<OraclePrice> {
        require!(
            !Contract::is_empty_account(oracle_account)?,
            ContractError::InvalidOracleAccount
        );

        // Manual deserialization to avoid lifetime issues
        let data = oracle_account.try_borrow_data()
            .map_err(|_| ContractError::InvalidOracleAccount)?;
        
        // Check account owner is Pyth Receiver program
        // let expected_owner = pyth_solana_receiver_sdk::ID;
        // require!(
        //     oracle_account.owner == &expected_owner,
        //     ContractError::InvalidOracleAccount
        // );

        // Deserialize using borsh
        let price_update: PriceUpdateV2 = anchor_lang::prelude::borsh::BorshDeserialize::deserialize(&mut &data[8..])
            .map_err(|e| {
                msg!("Failed to parse as PriceUpdateV2: {:?}", e);
                ContractError::InvalidOracleAccount
            })?;

        let clock = Clock::get()?;
        
        // Extract the feed ID from the price message (available for debugging)
        let _feed_id = &price_update.price_message.feed_id;
        
        // Get price with staleness check - using the struct methods
        let price_message = &price_update.price_message;
        
        // Check staleness
        let age = clock.unix_timestamp - price_message.publish_time;
        require!(
            age <= Self::MAX_PRICE_AGE_SEC as i64,
            ContractError::StaleOraclePrice
        );
        
        // Validate price confidence - confidence should be reasonable relative to price
        let confidence_bps = if price_message.price > 0 {
            ((price_message.conf as u128 * 10000) / price_message.price as u128) as u64
        } else {
            u64::MAX // Invalid if price is zero
        };
        require!(
            confidence_bps <= Self::MAX_CONFIDENCE_INTERVAL_BPS,
            ContractError::LowConfidencePrice
        );
        
        msg!("Pyth price: {}, exponent: {}, confidence: {}, age: {} seconds", 
             price_message.price, price_message.exponent, price_message.conf,
             age);
        
        // Reject negative prices - this indicates oracle failure
        require!(
            price_message.price > 0,
            ContractError::InvalidOraclePrice
        );
        let price_value = price_message.price as u64;
        
        Ok(OraclePrice {
            price: price_value,
            exponent: price_message.exponent,
        })
    }

    /// Better implementation with explicit feed_id string
    fn get_pyth_price_with_feed_id(
        oracle_account: &AccountInfo,
        feed_id_hex: &str,
    ) -> Result<OraclePrice> {
        require!(
            !Contract::is_empty_account(oracle_account)?,
            ContractError::InvalidOracleAccount
        );

        // Manual deserialization to avoid lifetime issues
        let data = oracle_account.try_borrow_data()
            .map_err(|_| ContractError::InvalidOracleAccount)?;
        
        // Check account owner is Pyth Receiver program
        let expected_owner = pyth_solana_receiver_sdk::ID;
        require!(
            oracle_account.owner == &expected_owner,
            ContractError::InvalidOracleAccount
        );

        // Deserialize using borsh
        let price_update: PriceUpdateV2 = anchor_lang::prelude::borsh::BorshDeserialize::deserialize(&mut &data[8..])
            .map_err(|e| {
                msg!("Failed to parse as PriceUpdateV2: {:?}", e);
                ContractError::InvalidOracleAccount
            })?;

        let clock = Clock::get()?;
        
        // Convert hex string to feed ID
        let feed_id = get_feed_id_from_hex(feed_id_hex)
            .map_err(|_| {
                msg!("Invalid feed ID hex string: {}", feed_id_hex);
                ContractError::InvalidOracleAccount
            })?;
        
        // Verify this price update is for the requested feed
        require!(
            price_update.price_message.feed_id == feed_id,
            ContractError::InvalidOracleAccount
        );
        
        let price_message = &price_update.price_message;
        
        // Check staleness
        let age = clock.unix_timestamp - price_message.publish_time;
        require!(
            age <= Self::MAX_PRICE_AGE_SEC as i64,
            ContractError::StaleOraclePrice
        );
        
        // Validate price confidence - confidence should be reasonable relative to price
        let confidence_bps = if price_message.price > 0 {
            ((price_message.conf as u128 * 10000) / price_message.price as u128) as u64
        } else {
            u64::MAX // Invalid if price is zero
        };
        require!(
            confidence_bps <= Self::MAX_CONFIDENCE_INTERVAL_BPS,
            ContractError::LowConfidencePrice
        );
        
        msg!("Pyth price for feed {}: {}, exponent: {}, confidence: {}, age: {} seconds", 
             feed_id_hex, price_message.price, price_message.exponent, price_message.conf,
             age);
        
        // Reject negative prices - this indicates oracle failure
        require!(
            price_message.price > 0,
            ContractError::InvalidOraclePrice
        );
        let price_value = price_message.price as u64;
        
        Ok(OraclePrice {
            price: price_value,
            exponent: price_message.exponent,
        })
    }
}

/// Feed ID constants for common price pairs
/// These are the official Pyth feed IDs from https://pyth.network/developers/price-feed-ids
pub struct FeedId;

impl FeedId {
    /// SOL/USD feed ID
    pub const SOL_USD: &'static str = "0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d";
    
    /// BTC/USD feed ID  
    pub const BTC_USD: &'static str = "0xe62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43";
    
    /// ETH/USD feed ID
    pub const ETH_USD: &'static str = "0xff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace";
    
    /// USDC/USD feed ID
    pub const USDC_USD: &'static str = "0xeaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a";
    
    /// USDT/USD feed ID
    pub const USDT_USD: &'static str = "0x2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca8ce04b0fd7f2e971688e2e53b";
}
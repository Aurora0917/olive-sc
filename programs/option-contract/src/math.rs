use anchor_lang::prelude::*;
use std::fmt::Display;

use crate::errors::MathError;

pub fn checked_add<T>(arg1: T, arg2: T) -> Result<T>
where
    T: num_traits::PrimInt + Display,
{
    if let Some(res) = arg1.checked_add(&arg2) {
        Ok(res)
    } else {
        msg!("Error: Overflow in {} + {}", arg1, arg2);
        err!(MathError::MathOverflow)
    }
}

pub fn checked_sub<T>(arg1: T, arg2: T) -> Result<T>
where
    T: num_traits::PrimInt + Display,
{
    if let Some(res) = arg1.checked_sub(&arg2) {
        Ok(res)
    } else {
        msg!("Error: Overflow in {} - {}", arg1, arg2);
        err!(MathError::MathOverflow)
    }
}

pub fn checked_div<T>(arg1: T, arg2: T) -> Result<T>
where
    T: num_traits::PrimInt + Display,
{
    if let Some(res) = arg1.checked_div(&arg2) {
        Ok(res)
    } else {
        msg!("Error: Overflow in {} / {}", arg1, arg2);
        err!(MathError::MathOverflow)
    }
}

pub fn checked_float_div<T>(arg1: T, arg2: T) -> Result<T>
where
    T: num_traits::Float + Display,
{
    if arg2 == T::zero() {
        msg!("Error: Overflow in {} / {}", arg1, arg2);
        return err!(MathError::MathOverflow);
    }
    let res = arg1 / arg2;
    if !res.is_finite() {
        msg!("Error: Overflow in {} / {}", arg1, arg2);
        err!(MathError::MathOverflow)
    } else {
        Ok(res)
    }
}

pub fn checked_as_u64<T>(arg: T) -> Result<u64>
where
    T: Display + num_traits::ToPrimitive + Clone,
{
    let option: Option<u64> = num_traits::NumCast::from(arg.clone());
    if let Some(res) = option {
        Ok(res)
    } else {
        msg!("Error: Overflow in {} as u64", arg);
        err!(MathError::MathOverflow)
    }
}

pub fn checked_mul<T>(arg1: T, arg2: T) -> Result<T>
where
    T: num_traits::PrimInt + Display,
{
    if let Some(res) = arg1.checked_mul(&arg2) {
        Ok(res)
    } else {
        msg!("Error: Overflow in {} * {}", arg1, arg2);
        err!(MathError::MathOverflow)
    }
}

pub fn checked_pow<T>(arg: T, exp: usize) -> Result<T>
where
    T: num_traits::PrimInt + Display,
{
    if let Some(res) = num_traits::checked_pow(arg, exp) {
        Ok(res)
    } else {
        msg!("Error: Overflow in {} ^ {}", arg, exp);
        err!(MathError::MathOverflow)
    }
}

pub fn checked_float_mul<T>(arg1: T, arg2: T) -> Result<T>
where
    T: num_traits::Float + Display,
{
    let res = arg1 * arg2;
    if !res.is_finite() {
        msg!("Error: Overflow in {} * {}", arg1, arg2);
        err!(MathError::MathOverflow)
    } else {
        Ok(res)
    }
}

pub fn checked_float_add<T>(arg1: T, arg2: T) -> Result<T>
where
    T: num_traits::Float + Display,
{
    let result = arg1 + arg2;
    if result.is_finite() {
        Ok(result)
    } else {
        msg!("Math error: checked_float_add overflow {} + {}", arg1, arg2);
        Err(error!(crate::errors::MathError::MathOverflow))
    }
}

pub fn checked_float_sub<T>(arg1: T, arg2: T) -> Result<T>
where
    T: num_traits::Float + Display,
{
    let result = arg1 - arg2;
    if result.is_finite() {
        Ok(result)
    } else {
        msg!("Math error: checked_float_sub overflow {} - {}", arg1, arg2);
        Err(error!(crate::errors::MathError::MathOverflow))
    }
}

pub fn checked_as_f64<T>(arg: T) -> Result<f64>
where
    T: Display + num_traits::ToPrimitive + Clone,
{
    let option: Option<f64> = num_traits::NumCast::from(arg.clone());
    if let Some(res) = option {
        Ok(res)
    } else {
        msg!("Error: Overflow in {} as f64", arg);
        err!(MathError::MathOverflow)
    }
}

pub fn checked_powi(arg: f64, exp: i32) -> Result<f64> {
    let res = if exp > 0 {
        f64::powi(arg, exp)
    } else {
        // wrokaround due to f64::powi() not working properly on-chain with negative exponent
        checked_float_div(1.0, f64::powi(arg, -exp))?
    };
    if res.is_finite() {
        Ok(res)
    } else {
        msg!("Error: Overflow in {} ^ {}", arg, exp);
        err!(MathError::MathOverflow)
    }
}

pub fn checked_decimal_mul(
    coefficient1: u64,
    exponent1: i32,
    coefficient2: u64,
    exponent2: i32,
    target_exponent: i32,
) -> Result<u64> {
    if coefficient1 == 0 || coefficient2 == 0 {
        return Ok(0);
    }
    let target_power = checked_sub(checked_add(exponent1, exponent2)?, target_exponent)?;
    if target_power >= 0 {
        checked_as_u64(checked_mul(
            checked_mul(coefficient1 as u128, coefficient2 as u128)?,
            checked_pow(10u128, target_power as usize)?,
        )?)
    } else {
        checked_as_u64(checked_div(
            checked_mul(coefficient1 as u128, coefficient2 as u128)?,
            checked_pow(10u128, (-target_power) as usize)?,
        )?)
    }
}

pub fn checked_decimal_div(
    coefficient1: u64,
    exponent1: i32,
    coefficient2: u64,
    exponent2: i32,
    target_exponent: i32,
) -> Result<u64> {
    if coefficient2 == 0 {
        msg!("Error: Overflow in {} / {}", coefficient1, coefficient2);
        return err!(MathError::MathOverflow);
    }
    if coefficient1 == 0 {
        return Ok(0);
    }
    // compute scale factor for the dividend
    let mut scale_factor = 0;
    let mut target_power = checked_sub(checked_sub(exponent1, exponent2)?, target_exponent)?;
    if exponent1 > 0 {
        scale_factor = checked_add(scale_factor, exponent1)?;
    }
    if exponent2 < 0 {
        scale_factor = checked_sub(scale_factor, exponent2)?;
        target_power = checked_add(target_power, exponent2)?;
    }
    if target_exponent < 0 {
        scale_factor = checked_sub(scale_factor, target_exponent)?;
        target_power = checked_add(target_power, target_exponent)?;
    }
    let scaled_coeff1 = if scale_factor > 0 {
        checked_mul(
            coefficient1 as u128,
            checked_pow(10u128, scale_factor as usize)?,
        )?
    } else {
        coefficient1 as u128
    };

    if target_power >= 0 {
        checked_as_u64(checked_mul(
            checked_div(scaled_coeff1, coefficient2 as u128)?,
            checked_pow(10u128, target_power as usize)?,
        )?)
    } else {
        checked_as_u64(checked_div(
            checked_div(scaled_coeff1, coefficient2 as u128)?,
            checked_pow(10u128, (-target_power) as usize)?,
        )?)
    }
}

pub fn checked_ceil_div<T>(arg1: T, arg2: T) -> Result<T>
where
    T: num_traits::PrimInt + Display,
{
    if arg1 > T::zero() {
        if arg1 == arg2 && arg2 != T::zero() {
            return Ok(T::one());
        }
        if let Some(res) = (arg1 - T::one()).checked_div(&arg2) {
            Ok(res + T::one())
        } else {
            msg!("Error: Overflow in {} / {}", arg1, arg2);
            err!(MathError::MathOverflow)
        }
    } else if let Some(res) = arg1.checked_div(&arg2) {
        Ok(res)
    } else {
        msg!("Error: Overflow in {} / {}", arg1, arg2);
        err!(MathError::MathOverflow)
    }
}

pub fn checked_decimal_ceil_mul(
    coefficient1: u64,
    exponent1: i32,
    coefficient2: u64,
    exponent2: i32,
    target_exponent: i32,
) -> Result<u64> {
    if coefficient1 == 0 || coefficient2 == 0 {
        return Ok(0);
    }
    let target_power = checked_sub(checked_add(exponent1, exponent2)?, target_exponent)?;
    if target_power >= 0 {
        checked_as_u64(checked_mul(
            checked_mul(coefficient1 as u128, coefficient2 as u128)?,
            checked_pow(10u128, target_power as usize)?,
        )?)
    } else {
        checked_as_u64(checked_ceil_div(
            checked_mul(coefficient1 as u128, coefficient2 as u128)?,
            checked_pow(10u128, (-target_power) as usize)?,
        )?)
    }
}

pub fn scale_to_exponent(arg: u64, exponent: i32, target_exponent: i32) -> Result<u64> {
    if target_exponent == exponent {
        return Ok(arg);
    }
    let delta = checked_sub(target_exponent, exponent)?;
    if delta > 0 {
        checked_div(arg, checked_pow(10, delta as usize)?)
    } else {
        checked_mul(arg, checked_pow(10, (-delta) as usize)?)
    }
}

// ===== FIXED-POINT PRICE SCALING UTILITIES =====
// These utilities handle conversion between f64 and scaled u64 for on-chain storage

/// Scaling factor for 6-decimal precision (1,000,000)
pub const PRICE_SCALE: u64 = 1_000_000;
pub const PRICE_DECIMALS: i32 = -6;

/// Maximum safe f64 value that can be scaled to u64 without overflow
/// u64::MAX / PRICE_SCALE = 18,446,744,073,709.551615
pub const MAX_SAFE_PRICE_F64: f64 = 18_446_744_073_709.0;

/// Converts f64 price to scaled u64 for on-chain storage
/// Uses 6 decimal precision (multiply by 1,000,000)
pub fn f64_to_scaled_price(price: f64) -> Result<u64> {
    if !price.is_finite() || price < 0.0 {
        msg!("Error: Invalid price value: {}", price);
        return err!(MathError::MathOverflow);
    }
    
    if price > MAX_SAFE_PRICE_F64 {
        msg!("Error: Price {} exceeds maximum safe value {}", price, MAX_SAFE_PRICE_F64);
        return err!(MathError::MathOverflow);
    }
    
    let scaled = checked_float_mul(price, PRICE_SCALE as f64)?;
    checked_as_u64(scaled.round())
}

/// Converts scaled u64 back to f64 for calculations
/// Divides by 1,000,000 to get original precision
pub fn scaled_price_to_f64(scaled_price: u64) -> Result<f64> {
    checked_float_div(scaled_price as f64, PRICE_SCALE as f64)
}

/// Converts f64 percentage/ratio to scaled u64 (e.g., 0.75 -> 750000 for 75%)
pub fn f64_to_scaled_ratio(ratio: f64) -> Result<u64> {
    if !ratio.is_finite() || ratio < 0.0 {
        msg!("Error: Invalid ratio value: {}", ratio);
        return err!(MathError::MathOverflow);
    }
    
    if ratio > MAX_SAFE_PRICE_F64 {
        msg!("Error: Ratio {} exceeds maximum safe value {}", ratio, MAX_SAFE_PRICE_F64);
        return err!(MathError::MathOverflow);
    }
    
    let scaled = checked_float_mul(ratio, PRICE_SCALE as f64)?;
    checked_as_u64(scaled.round())
}

/// Converts scaled u64 ratio back to f64
pub fn scaled_ratio_to_f64(scaled_ratio: u64) -> Result<f64> {
    checked_float_div(scaled_ratio as f64, PRICE_SCALE as f64)
}

/// Multiplies two scaled values and returns scaled result
/// Example: scaled_price * scaled_ratio = scaled_result
pub fn scaled_mul(val1: u64, val2: u64) -> Result<u64> {
    let result = checked_mul(val1 as u128, val2 as u128)?;
    checked_as_u64(checked_div(result, PRICE_SCALE as u128)?)
}

/// Divides two scaled values and returns scaled result  
/// Example: scaled_numerator / scaled_denominator = scaled_result
pub fn scaled_div(numerator: u64, denominator: u64) -> Result<u64> {
    if denominator == 0 {
        msg!("Error: Division by zero in scaled_div");
        return err!(MathError::MathOverflow);
    }
    
    let scaled_numerator = checked_mul(numerator as u128, PRICE_SCALE as u128)?;
    checked_as_u64(checked_div(scaled_numerator, denominator as u128)?)
}

/// Constants for common scaled values
pub const SCALED_ONE: u64 = PRICE_SCALE; // 1.0 in scaled format
pub const SCALED_ZERO: u64 = 0; // 0.0 in scaled format
pub const SCALED_HUNDRED: u64 = 100 * PRICE_SCALE; // 100.0 in scaled format

/// Converts scaled percentage to basis points (for compatibility with existing BPS code)
pub fn scaled_to_bps(scaled_pct: u64) -> Result<u32> {
    // scaled_pct is already percentage * 1_000_000
    // Convert to basis points by dividing by 100 (since 1% = 100 BPS)
    let bps = checked_div(scaled_pct, 10_000)?; // 1_000_000 / 100 = 10_000
    if bps > u32::MAX as u64 {
        msg!("Error: BPS value {} exceeds u32::MAX", bps);
        return err!(MathError::MathOverflow);
    }
    Ok(bps as u32)
}

/// Converts basis points to scaled percentage
pub fn bps_to_scaled(bps: u32) -> Result<u64> {
    checked_mul(bps as u64, 10_000) // Convert BPS to scaled percentage
}

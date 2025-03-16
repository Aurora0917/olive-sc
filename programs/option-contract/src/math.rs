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
        err!(MathError::OverflowMathError)
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
        err!(MathError::OverflowMathError)
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
        err!(MathError::OverflowMathError)
    }
}

pub fn checked_float_div<T>(arg1: T, arg2: T) -> Result<T>
where
    T: num_traits::Float + Display,
{
    if arg2 == T::zero() {
        msg!("Error: Overflow in {} / {}", arg1, arg2);
        return err!(MathError::OverflowMathError);
    }
    let res = arg1 / arg2;
    if !res.is_finite() {
        msg!("Error: Overflow in {} / {}", arg1, arg2);
        err!(MathError::OverflowMathError)
    } else {
        Ok(res)
    }
}
use anchor_lang::prelude::*;

pub const FULL_BPS: u32 = 10000;

#[derive(
    AnchorSerialize, 
    AnchorDeserialize, 
    Clone, 
    Copy, 
    Debug, 
    PartialEq, 
    PartialOrd,
    InitSpace
)]
pub struct Fraction {
    pub value: u128,
}

impl Fraction {
    pub const ONE: Fraction = Fraction { value: FULL_BPS as u128 };
    pub const ZERO: Fraction = Fraction { value: 0 };

    pub fn from_bps(bps: u32) -> Self {
        Self { value: bps as u128 }
    }

    pub fn to_bps(&self) -> Option<u32> {
        if self.value <= u32::MAX as u128 {
            Some(self.value as u32)
        } else {
            None
        }
    }

    pub fn to_bits(&self) -> u128 {
        self.value
    }

    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.value.checked_sub(other.value).map(|v| Fraction { value: v })
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.value.checked_add(other.value).map(|v| Fraction { value: v })
    }

    pub fn checked_mul(self, scalar: u128) -> Option<Self> {
        self.value.checked_mul(scalar).map(|v| Fraction { value: v })
    }

    pub fn checked_div(self, scalar: u128) -> Option<Self> {
        if scalar == 0 {
            None
        } else {
            Some(Fraction { value: self.value / scalar })
        }
    }
}

impl std::ops::Add for Fraction {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Fraction { value: self.value + other.value }
    }
}

impl std::ops::Sub for Fraction {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        // Use checked_sub to prevent underflow, falling back to 0
        Fraction { value: self.value.checked_sub(other.value).unwrap_or(0) }
    }
}

impl std::ops::Mul<u128> for Fraction {
    type Output = Self;
    fn mul(self, scalar: u128) -> Self {
        // Use saturating_mul to prevent overflow
        Fraction { value: self.value.saturating_mul(scalar) }
    }
}

impl std::ops::Div<u128> for Fraction {
    type Output = Self;
    fn div(self, scalar: u128) -> Self {
        if scalar == 0 {
            Fraction { value: 0 }
        } else {
            Fraction { value: self.value / scalar }
        }
    }
}
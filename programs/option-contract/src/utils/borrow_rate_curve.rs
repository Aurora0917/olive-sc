use anchor_lang::prelude::*;
use crate::utils::*;
use crate::errors::OptionError;
use crate::errors::MathError;

pub const MAX_UTILIZATION_RATE_BPS: u32 = FULL_BPS;

#[derive(
    AnchorSerialize, 
    AnchorDeserialize,
    Clone, 
    Copy, 
    Debug, 
    Default, 
    PartialEq, 
    Eq,
    InitSpace
)]
pub struct CurvePoint {
    pub utilization_rate_bps: u32,
    pub borrow_rate_bps: u32,
}

impl CurvePoint {
    pub fn new(utilization_rate_bps: u32, borrow_rate_bps: u32) -> Self {
        Self {
            utilization_rate_bps,
            borrow_rate_bps,
        }
    }
}

#[derive(
    AnchorSerialize, 
    AnchorDeserialize,
    Clone, 
    Debug, 
    PartialEq, 
    Eq,
    InitSpace
)]
pub struct BorrowRateCurve {
    pub points: [CurvePoint; 11],
}

impl Default for BorrowRateCurve {
    fn default() -> Self {
        BorrowRateCurve::new_flat(0)
    }
}

impl BorrowRateCurve {
    pub fn validate(&self) -> Result<()> {
        let pts = &self.points;

        require!(
            pts[0].utilization_rate_bps == 0,
            OptionError::InvalidBorrowRateCurvePoint
        );

        require!(
            pts[10].utilization_rate_bps == MAX_UTILIZATION_RATE_BPS,
            OptionError::InvalidBorrowRateCurvePoint
        );

        let mut last_pt = pts[0];
        for pt in pts.iter().skip(1) {
            if last_pt.utilization_rate_bps == MAX_UTILIZATION_RATE_BPS {
                require!(
                    pt.utilization_rate_bps == MAX_UTILIZATION_RATE_BPS,
                    OptionError::InvalidBorrowRateCurvePoint
                );
            } else {
                require!(
                    pt.utilization_rate_bps > last_pt.utilization_rate_bps,
                    OptionError::InvalidBorrowRateCurvePoint
                );
            }
            
            require!(
                pt.borrow_rate_bps >= last_pt.borrow_rate_bps,
                OptionError::InvalidBorrowRateCurvePoint
            );
            
            last_pt = *pt;
        }
        Ok(())
    }

    pub fn from_points(pts: &[CurvePoint]) -> Result<Self> {
        require!(pts.len() >= 2, OptionError::InvalidBorrowRateCurvePoint);
        require!(pts.len() <= 11, OptionError::InvalidBorrowRateCurvePoint);
        
        let last = pts.last().unwrap();
        require!(
            last.utilization_rate_bps == MAX_UTILIZATION_RATE_BPS,
            OptionError::InvalidBorrowRateCurvePoint
        );

        let mut points = [*last; 11];
        points[..pts.len()].copy_from_slice(pts);

        let curve = BorrowRateCurve { points };
        curve.validate()?;
        Ok(curve)
    }

    pub fn new_flat(borrow_rate_bps: u32) -> Self {
        let points = [
            CurvePoint::new(0, borrow_rate_bps),
            CurvePoint::new(MAX_UTILIZATION_RATE_BPS, borrow_rate_bps),
        ];
        Self::from_points(&points).unwrap()
    }

    pub fn from_legacy_parameters(
        optimal_utilization_rate_pct: u8,
        base_rate_pct: u8,
        optimal_rate_pct: u8,
        max_rate_pct: u8,
    ) -> Self {
        let optimal_utilization_rate = u32::from(optimal_utilization_rate_pct) * 100;
        let base_rate = u32::from(base_rate_pct) * 100;
        let optimal_rate = u32::from(optimal_rate_pct) * 100;
        let max_rate = u32::from(max_rate_pct) * 100;

        let points: &[CurvePoint] = if optimal_utilization_rate == 0 {
            &[
                CurvePoint::new(0, optimal_rate),
                CurvePoint::new(MAX_UTILIZATION_RATE_BPS, max_rate),
            ]
        } else if optimal_utilization_rate == MAX_UTILIZATION_RATE_BPS {
            &[
                CurvePoint::new(0, base_rate),
                CurvePoint::new(MAX_UTILIZATION_RATE_BPS, optimal_rate),
            ]
        } else {
            &[
                CurvePoint::new(0, base_rate),
                CurvePoint::new(optimal_utilization_rate, optimal_rate),
                CurvePoint::new(MAX_UTILIZATION_RATE_BPS, max_rate),
            ]
        };
        Self::from_points(points).unwrap()
    }

    pub fn get_borrow_rate(&self, utilization_rate: Fraction) -> Result<Fraction> {
        let utilization_rate = if utilization_rate > Fraction::ONE {
            Fraction::ONE
        } else {
            utilization_rate
        };

        let utilization_rate_bps = utilization_rate
            .to_bps()
            .ok_or(MathError::OverflowMathError)?;

        for window in self.points.windows(2) {
            let start_pt = window[0];
            let end_pt = window[1];

            if utilization_rate_bps >= start_pt.utilization_rate_bps
                && utilization_rate_bps <= end_pt.utilization_rate_bps
            {
                if utilization_rate_bps == start_pt.utilization_rate_bps {
                    return Ok(Fraction::from_bps(start_pt.borrow_rate_bps));
                }
                if utilization_rate_bps == end_pt.utilization_rate_bps {
                    return Ok(Fraction::from_bps(end_pt.borrow_rate_bps));
                }

                return self.interpolate(start_pt, end_pt, utilization_rate);
            }
        }

        err!(OptionError::InvalidUtilizationRate)
    }

    fn interpolate(&self, start_pt: CurvePoint, end_pt: CurvePoint, utilization_rate: Fraction) -> Result<Fraction> {
        let slope_nom = end_pt.borrow_rate_bps
            .checked_sub(start_pt.borrow_rate_bps)
            .ok_or(OptionError::InvalidBorrowRateCurvePoint)?;

        let slope_denom = end_pt.utilization_rate_bps
            .checked_sub(start_pt.utilization_rate_bps)
            .ok_or(OptionError::InvalidBorrowRateCurvePoint)?;

        let start_utilization_rate = Fraction::from_bps(start_pt.utilization_rate_bps);
        let coef = utilization_rate
            .checked_sub(start_utilization_rate)
            .ok_or(OptionError::InvalidUtilizationRate)?;

        let nom = coef
            .checked_mul(slope_nom as u128)
            .ok_or(MathError::OverflowMathError)?;
        let base_rate = nom
            .checked_div(slope_denom as u128)
            .ok_or(MathError::OverflowMathError)?;

        let offset = Fraction::from_bps(start_pt.borrow_rate_bps);
        base_rate
            .checked_add(offset)
            .ok_or(MathError::OverflowMathError.into())
    }
}
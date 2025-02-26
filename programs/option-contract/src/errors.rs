use anchor_lang::error_code;

#[error_code]
pub enum OptionError {
    InvalidPoolBalanceError,
    InvalidLockedBalanceError,
    InvalidSignerBalanceError,
    InvalidOptionIndexError,
    InvalidTimeError,
    InvalidPriceRequirementError,
}

#[error_code]
pub enum PoolError {
    InvalidWithdrawError,
    InvalidPoolBalanceError,
    InvalidSignerBalanceError,
}

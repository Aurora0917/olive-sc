use anchor_lang::error_code;

#[error_code]
pub enum OptionError {
    InvalidPoolBalanceError,
    InvalidLockedBalanceError,
    InvalidSignerBalanceError,
    InvalidOptionIndexError,
    InvalidTimeError,
    InvalidPriceRequirementError,
    StalePriceError,
}

#[error_code]
pub enum PoolError {
    InvalidWithdrawError,
    InvalidPoolBalanceError,
    InvalidSignerBalanceError,
}

#[error_code]
pub enum MultiSigError {
    #[msg("Account is not authorized to sign this instruction")]
    NotAuthorizedMultiSigError,
    AlreadySignedMultiSigError,
    AlreadyExecutedMultiSigError,
}

#[error_code]
pub enum MathError {
    #[msg("Overflow in arithmetic operation")]
    OverflowMathError,
}
use anchor_lang::error_code;

#[error_code]
pub enum OptionError {
    InvalidPoolBalanceError,
    InvalidLockedBalanceError,
    InvalidSignerBalanceError,
    InvalidOptionIndexError,
    InvalidTimeError,
    InvalidMintError,
    InvalidPriceRequirementError,
    StalePriceError,
    InvalidPayAmountError,
    InvalidOwner,
    OptionNotValid,
    OptionAlreadyExercised,
    InsufficientFundsError,
    InvalidQuantityError,
    InsufficientQuantityError,
    BorrowRateNotInitialized,
    AssetTypeNotSet,
    InvalidPoolIndex,
    UtilizationTooHigh,
    InvalidUtilizationRate,
    InvalidBorrowRateCurvePoint,
    InsufficientPoolLiquidity,
    InsufficientBalance,
    InvalidAmount,
    InvalidLeverage,
    InvalidSlippage,
    MathOverflow,
    PriceSlippage,
    PositionLiquidated,
    Unauthorized,
    InvalidLiquidationPrice,
    InvalidPoolName,
    InsufficientCollateral,
    InvalidCollateralAsset,
    PositionNotLiquidatable,
    SlippageExceededError,
    InvalidParameterError,
    OptionExpired
}

#[error_code]
pub enum PoolError {
    InvalidWithdrawError,
    InvalidPoolBalanceError,
    InvalidSignerBalanceError,
    InvalidCustodyTokenError,
    InvalidPoolState,
    InvalidCustodyState
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

    #[msg("Invalid mint address")]
    InvalidMint,
    
    #[msg("Invalid owner")]
    InvalidOwner,
}


#[error_code]
pub enum ContractError {
    #[msg("Oracle Account is invalid")]
    InvalidOracleAccount,
    #[msg("Stale oracle price")]
    StaleOraclePrice,
    InsufficientAmountReturned,
    TokenRatioOutOfRange,
    CustodyAmountLimit
}

#[error_code]
pub enum PerpetualsError {
    #[msg("Account is not authorized to sign this instruction")]
    MultisigAccountNotAuthorized,
    #[msg("Account has already signed this instruction")]
    MultisigAlreadySigned,
    #[msg("This instruction has already been executed")]
    MultisigAlreadyExecuted,
    #[msg("Overflow in arithmetic operation")]
    MathOverflow,
    #[msg("Unsupported price oracle")]
    UnsupportedOracle,
    #[msg("Invalid oracle account")]
    InvalidOracleAccount,
    #[msg("Invalid oracle state")]
    InvalidOracleState,
    #[msg("Stale oracle price")]
    StaleOraclePrice,
    #[msg("Invalid oracle price")]
    InvalidOraclePrice,
    #[msg("Instruction is not allowed in production")]
    InvalidEnvironment,
    #[msg("Invalid pool state")]
    InvalidPoolState,
    #[msg("Invalid custody state")]
    InvalidCustodyState,
    #[msg("Invalid collateral custody")]
    InvalidCollateralCustody,
    #[msg("Invalid position state")]
    InvalidPositionState,
    #[msg("Invalid perpetuals config")]
    InvalidPerpetualsConfig,
    #[msg("Invalid pool config")]
    InvalidPoolConfig,
    #[msg("Invalid custody config")]
    InvalidCustodyConfig,
    #[msg("Insufficient token amount returned")]
    InsufficientAmountReturned,
    #[msg("Price slippage limit exceeded")]
    MaxPriceSlippage,
    #[msg("Position leverage limit exceeded")]
    MaxLeverage,
    #[msg("Custody amount limit exceeded")]
    CustodyAmountLimit,
    #[msg("Position amount limit exceeded")]
    PositionAmountLimit,
    #[msg("Token ratio out of range")]
    TokenRatioOutOfRange,
    #[msg("Token is not supported")]
    UnsupportedToken,
    #[msg("Instruction is not allowed at this time")]
    InstructionNotAllowed,
    #[msg("Token utilization limit exceeded")]
    MaxUtilization,
    #[msg("Permissionless oracle update must be preceded by Ed25519 signature verification instruction")]
    PermissionlessOracleMissingSignature,
    #[msg("Ed25519 signature verification data does not match expected format")]
    PermissionlessOracleMalformedEd25519Data,
    #[msg("Ed25519 signature was not signed by the oracle authority")]
    PermissionlessOracleSignerMismatch,
    #[msg("Signed message does not match instruction params")]
    PermissionlessOracleMessageMismatch,
}
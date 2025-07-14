use anchor_lang::error_code;

#[error_code]
pub enum OptionError {
    // Option-specific errors
    #[msg("Option is not valid or has expired")]
    OptionNotValid,
    #[msg("Option has already been exercised")]
    OptionAlreadyExercised,
    #[msg("Option has expired")]
    OptionExpired,
    #[msg("Option expired or invalid timing")]
    InvalidTimeError,
    #[msg("Option index out of range or invalid")]
    InvalidOptionIndexError,
    #[msg("Invalid option owner")]
    InvalidOwner,
    #[msg("Zero quantity not allowed for options - check premium calculation")]
    ZeroQuantityError,
    #[msg("Invalid quantity specified")]
    InvalidQuantityError,
    #[msg("Insufficient quantity available")]
    InsufficientQuantityError,
    #[msg("Invalid pay amount calculated")]
    InvalidPayAmountError,
    #[msg("Invalid mint specified")]
    InvalidMintError,
    #[msg("Price requirement not met for option exercise")]
    InvalidPriceRequirementError,
    #[msg("Insufficient balance to cover option premium")]
    InvalidSignerBalanceError,
    #[msg("Invalid locked balance in custody")]
    InvalidLockedBalanceError,
    #[msg("Invalid pool balance state")]
    InvalidPoolBalanceError,
    #[msg("Price confidence interval too wide - oracle data unreliable")]
    PriceConfidenceError,
    #[msg("Precision loss detected in calculations - values too small")]
    PrecisionLossError,
    #[msg("Invalid parameter provided")]
    InvalidParameterError,
    #[msg("Invalid leverage specified")]
    InvalidLeverage,
    #[msg("Insufficient collateral")]
    InsufficientCollateral,
    #[msg("Position has been liquidated")]
    PositionLiquidated,
    #[msg("Position cannot be liquidated")]
    PositionNotLiquidatable,
    #[msg("Slippage exceeded on trade")]
    SlippageExceededError,
    #[msg("Invalid amount specified")]
    InvalidAmount,
    #[msg("Invalid slippage tolerance")]
    InvalidSlippage,
    #[msg("Invalid pool name")]
    InvalidPoolName,
    #[msg("Insufficient balance for operation")]
    InsufficientBalance,
    #[msg("Invalid liquidation price")]
    InvalidLiquidationPrice,
    #[msg("Price slippage exceeded limits")]
    PriceSlippage,
    #[msg("Unauthorized access")]
    Unauthorized,
    #[msg("Insufficient pool liquidity")]
    InsufficientPoolLiquidity,
}

#[error_code]
pub enum PoolError {
    #[msg("Invalid withdrawal amount or conditions")]
    InvalidWithdrawError,
    #[msg("Pool balance is invalid or insufficient")]
    InvalidPoolBalanceError,
    #[msg("Signer balance insufficient for pool operation")]
    InvalidSignerBalanceError,
    #[msg("Invalid custody token specified")]
    InvalidCustodyTokenError,
    #[msg("Pool is in invalid state")]
    InvalidPoolState,
    #[msg("Custody is in invalid state")]
    InvalidCustodyState,
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
    #[msg("Division by zero attempted")]
    DivisionByZero,
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
    #[msg("Invalid oracle price - negative or zero price detected")]
    InvalidOraclePrice,
    #[msg("Oracle price confidence too low")]
    LowConfidencePrice,
    #[msg("Insufficient amount returned from operation")]
    InsufficientAmountReturned,
    #[msg("Token ratio is out of acceptable range")]
    TokenRatioOutOfRange,
    #[msg("Custody amount limit exceeded")]
    CustodyAmountLimit,
}

// Add new error categories for better organization
#[error_code]
pub enum TradingError {
    #[msg("Invalid amount specified")]
    InvalidAmount,
    #[msg("Invalid leverage specified")]
    InvalidLeverage,
    #[msg("Invalid slippage tolerance")]
    InvalidSlippage,
    #[msg("Price slippage exceeded limits")]
    PriceSlippage,
    #[msg("Slippage exceeded on trade")]
    SlippageExceededError,
    #[msg("Invalid parameter provided")]
    InvalidParameterError,
    #[msg("Insufficient balance for operation")]
    InsufficientBalance,
    #[msg("Insufficient funds for transaction")]
    InsufficientFundsError,
    #[msg("Insufficient pool liquidity")]
    InsufficientPoolLiquidity,
    #[msg("Insufficient collateral")]
    InsufficientCollateral,
    #[msg("Invalid collateral asset")]
    InvalidCollateralAsset,
    #[msg("Invalid pool name")]
    InvalidPoolName,
    #[msg("Invalid pool index")]
    InvalidPoolIndex,
    #[msg("Unauthorized access")]
    Unauthorized,
    #[msg("Math overflow in calculation")]
    MathOverflow,
    #[msg("Stale price data")]
    StalePriceError,
    #[msg("Invalid take profit price")]
    InvalidTakeProfitPrice,
    #[msg("Invalid stop loss price")]
    InvalidStopLossPrice,
}

#[error_code]
pub enum PerpetualsError {
    // Perpetual position errors
    #[msg("Position has been liquidated")]
    PositionLiquidated,
    #[msg("Position cannot be liquidated")]
    PositionNotLiquidatable,
    #[msg("Invalid liquidation price")]
    InvalidLiquidationPrice,
    #[msg("Utilization rate too high")]
    UtilizationTooHigh,
    #[msg("Invalid utilization rate")]
    InvalidUtilizationRate,
    #[msg("Invalid borrow rate curve point")]
    InvalidBorrowRateCurvePoint,
    #[msg("Borrow rate not initialized")]
    BorrowRateNotInitialized,
    #[msg("Asset type not set")]
    AssetTypeNotSet,
    
    // Keep existing perpetual errors
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
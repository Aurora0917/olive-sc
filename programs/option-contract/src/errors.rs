use anchor_lang::error_code;

// Option-specific errors only
#[error_code]
pub enum OptionError {
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
    #[msg("Zero quantity not allowed for options - check premium calculation")]
    ZeroQuantityError,
    #[msg("Invalid quantity specified")]
    InvalidQuantityError,
    #[msg("Insufficient quantity available")]
    InsufficientQuantityError,
    #[msg("Invalid pay amount calculated")]
    InvalidPayAmountError,
    #[msg("Price requirement not met for option exercise")]
    InvalidPriceRequirementError,
    #[msg("Option has been executed")]
    OptionExecuted,
    #[msg("Invalid option")]
    InvalidOption,
    #[msg("Invalid option premium calculation")]
    InvalidPremiumCalculation,
    #[msg("Option strike price is invalid")]
    InvalidStrikePrice,
    #[msg("Option expiry date is invalid")]
    InvalidExpiryDate,
    #[msg("Option cannot be closed at current price")]
    InvalidCloseCondition,
}

// Perpetual-specific errors only
#[error_code]
pub enum PerpetualError {
    #[msg("Position has been liquidated")]
    PositionLiquidated,
    #[msg("Position cannot be liquidated")]
    PositionNotLiquidatable,
    #[msg("Invalid liquidation price")]
    InvalidLiquidationPrice,
    #[msg("Invalid leverage specified")]
    InvalidLeverage,
    #[msg("Insufficient collateral")]
    InsufficientCollateral,
    #[msg("Invalid collateral asset")]
    InvalidCollateralAsset,
    #[msg("Would cause liquidation")]
    WouldCauseLiquidation,
    #[msg("Insufficient margin")]
    InsufficientMargin,
    #[msg("Invalid position type")]
    InvalidPositionType,
    #[msg("Position is not a limit order")]
    NotLimitOrder,
    #[msg("Position is not a market order")]
    NotMarketOrder,
    #[msg("Limit order cannot be executed at current price")]
    LimitOrderNotTriggered,
    #[msg("Position already executed")]
    PositionAlreadyExecuted,
    #[msg("Invalid trigger price")]
    InvalidTriggerPrice,
    #[msg("Invalid position size")]
    InvalidPositionSize,
    #[msg("Position size too small")]
    PositionSizeTooSmall,
    #[msg("Position size too large")]
    PositionSizeTooLarge,
    #[msg("Maximum leverage exceeded")]
    MaxLeverageExceeded,
    #[msg("Minimum margin requirement not met")]
    MinMarginNotMet,
    #[msg("Position funding payment failed")]
    FundingPaymentFailed,
    #[msg("Position interest payment failed")]
    InterestPaymentFailed,
    #[msg("Invalid funding rate")]
    InvalidFundingRate,
    #[msg("Invalid interest rate")]
    InvalidInterestRate,
    #[msg("Position PnL calculation failed")]
    PnLCalculationFailed,
    #[msg("Position update failed")]
    PositionUpdateFailed,
    #[msg("Invalid position state")]
    InvalidPositionState,
    #[msg("Position not found")]
    PositionNotFound,
    #[msg("Cannot modify executed position")]
    CannotModifyExecutedPosition,
    #[msg("Invalid execution price")]
    InvalidExecutionPrice,
    #[msg("Position cannot be canceled")]
    PositionCannotBeCanceled,
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
    #[msg("Invalid perpetuals config")]
    InvalidPerpetualsConfig,
    #[msg("Custody amount limit exceeded")]
    CustodyAmountLimit,
    #[msg("Position amount limit exceeded")]
    PositionAmountLimit,
    #[msg("Maximum utilization exceeded")]
    MaxUtilization,
}

// General trading errors that apply to both options and perpetuals
#[error_code]
pub enum TradingError {
    #[msg("Invalid amount specified")]
    InvalidAmount,
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
    #[msg("Unauthorized access")]
    Unauthorized,
    #[msg("Invalid owner")]
    InvalidOwner,
    #[msg("Invalid mint specified")]
    InvalidMintError,
    #[msg("Invalid price specified")]
    InvalidPrice,
    #[msg("Invalid price range for TP/SL")]
    InvalidPriceRange,
    #[msg("Invalid take profit price")]
    InvalidTakeProfitPrice,
    #[msg("Invalid stop loss price")]
    InvalidStopLossPrice,
    #[msg("Insufficient balance to cover premium/collateral")]
    InvalidSignerBalanceError,
    #[msg("Invalid locked balance in custody")]
    InvalidLockedBalanceError,
    #[msg("Price confidence interval too wide - oracle data unreliable")]
    PriceConfidenceError,
    #[msg("Precision loss detected in calculations - values too small")]
    PrecisionLossError,
    #[msg("Stale price data")]
    StalePriceError,
}

// Pool-specific errors
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
    #[msg("Invalid pool name")]
    InvalidPoolName,
    #[msg("Invalid pool index")]
    InvalidPoolIndex,
    #[msg("Invalid pool config")]
    InvalidPoolConfig,
    #[msg("Invalid custody config")]
    InvalidCustodyConfig,
    #[msg("Invalid collateral custody")]
    InvalidCollateralCustody,
    #[msg("Token ratio out of range")]
    TokenRatioOutOfRange,
    #[msg("Token is not supported")]
    UnsupportedToken,
}

// Contract-specific errors
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
    #[msg("Unsupported price oracle")]
    UnsupportedOracle,
    #[msg("Invalid oracle state")]
    InvalidOracleState,
    #[msg("Instruction is not allowed in production")]
    InvalidEnvironment,
    #[msg("Instruction is not allowed at this time")]
    InstructionNotAllowed,
    #[msg("Permissionless oracle update must be preceded by Ed25519 signature verification instruction")]
    PermissionlessOracleMissingSignature,
    #[msg("Ed25519 signature verification data does not match expected format")]
    PermissionlessOracleMalformedEd25519Data,
    #[msg("Ed25519 signature was not signed by the oracle authority")]
    PermissionlessOracleSignerMismatch,
    #[msg("Signed message does not match instruction params")]
    PermissionlessOracleMessageMismatch,
}

// Mathematical operation errors
#[error_code]
pub enum MathError {
    #[msg("Overflow in arithmetic operation")]
    MathOverflow,
    #[msg("Division by zero attempted")]
    DivisionByZero,
    #[msg("Underflow in arithmetic operation")]
    MathUnderflow,
    #[msg("Invalid calculation result")]
    InvalidCalculationResult,
    #[msg("Precision loss in calculation")]
    PrecisionLoss,
    #[msg("Number conversion failed")]
    ConversionFailed,
}

// Multi-signature errors
#[error_code]
pub enum MultiSigError {
    #[msg("Account is not authorized to sign this instruction")]
    NotAuthorizedMultiSigError,
    #[msg("Account has already signed this instruction")]
    AlreadySignedMultiSigError,
    #[msg("This instruction has already been executed")]
    AlreadyExecutedMultiSigError,
    #[msg("Multisig account not authorized")]
    MultisigAccountNotAuthorized,
    #[msg("Multisig already signed")]
    MultisigAlreadySigned,
    #[msg("Multisig already executed")]
    MultisigAlreadyExecuted,
}
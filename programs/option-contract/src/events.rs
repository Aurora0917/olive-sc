use anchor_lang::prelude::*;

// Option related events - containing ALL fields from msg! calls
#[event]
pub struct OptionOpened {
    pub owner: Pubkey,
    pub index: u64,
    pub amount: u64,
    pub quantity: u64,
    pub period: u64,
    pub expired_date: i64,
    pub purchase_date: u64,
    pub option_type: u8,
    pub strike_price: u64,
    pub valid: bool,
    pub locked_asset: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub premium: u64,
    pub premium_asset: Pubkey,
    pub limit_price: u64,
    pub executed: bool,
    pub entry_price: u64,
    pub last_update_time: i64,
    pub take_profit_price: Option<u64>,
    pub stop_loss_price: Option<u64>,
    pub bump: u8,
}

#[event]
pub struct OptionClosed {
    pub owner: Pubkey,
    pub index: u64,
    pub amount: u64,
    pub quantity: u64,
    pub period: u64,
    pub expired_date: i64,
    pub purchase_date: u64,
    pub option_type: u8,
    pub strike_price: u64,
    pub valid: bool,
    pub locked_asset: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub premium: u64,
    pub premium_asset: Pubkey,
    pub limit_price: u64,
    pub executed: bool,
    pub entry_price: u64,
    pub last_update_time: i64,
    pub take_profit_price: Option<u64>,
    pub stop_loss_price: Option<u64>,
    pub close_quantity: u64,
}

#[event]
pub struct OptionExercised {
    pub owner: Pubkey,
    pub index: u64,
    pub amount: u64,
    pub quantity: u64,
    pub period: u64,
    pub expired_date: i64,
    pub purchase_date: u64,
    pub option_type: u8,
    pub strike_price: u64,
    pub valid: bool,
    pub locked_asset: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub premium: u64,
    pub premium_asset: Pubkey,
    pub limit_price: u64,
    pub executed: bool,
    pub entry_price: u64,
    pub last_update_time: i64,
    pub take_profit_price: Option<u64>,
    pub stop_loss_price: Option<u64>,
    pub exercised: u64,
    pub profit: u64,
}

#[event]
pub struct OptionTpSlSet {
    pub owner: Pubkey,
    pub index: u64,
    pub amount: u64,
    pub quantity: u64,
    pub period: u64,
    pub expired_date: i64,
    pub purchase_date: u64,
    pub option_type: u8,
    pub strike_price: u64,
    pub valid: bool,
    pub locked_asset: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub premium: u64,
    pub premium_asset: Pubkey,
    pub limit_price: u64,
    pub executed: bool,
    pub entry_price: u64,
    pub last_update_time: i64,
    pub take_profit_price: Option<u64>,
    pub stop_loss_price: Option<u64>,
}

#[event]
pub struct LimitOptionOpened {
    pub owner: Pubkey,
    pub index: u64,
    pub amount: u64,
    pub quantity: u64,
    pub period: u64,
    pub expired_date: i64,
    pub purchase_date: u64,
    pub option_type: u8,
    pub strike_price: u64,
    pub valid: bool,
    pub locked_asset: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub premium: u64,
    pub premium_asset: Pubkey,
    pub limit_price: u64,
    pub executed: bool,
    pub entry_price: u64,
    pub last_update_time: i64,
    pub take_profit_price: Option<u64>,
    pub stop_loss_price: Option<u64>,
}

#[event]
pub struct LimitOptionClosed {
    pub owner: Pubkey,
    pub index: u64,
    pub amount: u64,
    pub quantity: u64,
    pub period: u64,
    pub expired_date: i64,
    pub purchase_date: u64,
    pub option_type: u8,
    pub strike_price: u64,
    pub valid: bool,
    pub locked_asset: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub premium: u64,
    pub premium_asset: Pubkey,
    pub limit_price: u64,
    pub executed: bool,
    pub entry_price: u64,
    pub last_update_time: i64,
    pub take_profit_price: Option<u64>,
    pub stop_loss_price: Option<u64>,
    pub close_quantity: u64,
}

// Perpetual position events - containing ALL fields from msg! calls
#[event]
pub struct PerpPositionOpened {
    pub index: u64,
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub collateral_custody: Pubkey,
    pub position_type: u8,
    pub side: u8,
    pub is_liquidated: bool,
    pub price: u64,
    pub size_usd: u64,
    pub borrow_size_usd: u64,
    pub collateral_usd: u64,
    pub open_time: i64,
    pub update_time: i64,
    pub liquidation_price: u64,
    pub cumulative_interest_snapshot: u128,
    pub opening_fee_paid: u64,
    pub total_fees_paid: u64,
    pub locked_amount: u64,
    pub collateral_amount: u64,
    pub take_profit_price: Option<u64>,
    pub stop_loss_price: Option<u64>,
    pub trigger_price: Option<u64>,
    pub trigger_above_threshold: bool,
    pub bump: u8,
}

#[event]
pub struct PerpPositionClosed {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub collateral_custody: Pubkey,
    pub position_type: u8,
    pub side: u8,
    pub is_liquidated: bool,
    pub price: u64,
    pub size_usd: u64,
    pub borrow_size_usd: u64,
    pub collateral_usd: u64,
    pub open_time: i64,
    pub update_time: i64,
    pub liquidation_price: u64,
    pub cumulative_interest_snapshot: u128,
    pub opening_fee_paid: u64,
    pub total_fees_paid: u64,
    pub locked_amount: u64,
    pub collateral_amount: u64,
    pub take_profit_price: Option<u64>,
    pub stop_loss_price: Option<u64>,
    pub trigger_price: Option<u64>,
    pub trigger_above_threshold: bool,
    pub bump: u8,
    pub close_percentage: u64,
    pub settlement_tokens: u64,
}

#[event]
pub struct PerpTpSlSet {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub collateral_custody: Pubkey,
    pub position_type: u8,
    pub side: u8,
    pub is_liquidated: bool,
    pub price: u64,
    pub size_usd: u64,
    pub borrow_size_usd: u64,
    pub collateral_usd: u64,
    pub open_time: i64,
    pub update_time: i64,
    pub liquidation_price: u64,
    pub cumulative_interest_snapshot: u128,
    pub opening_fee_paid: u64,
    pub total_fees_paid: u64,
    pub locked_amount: u64,
    pub collateral_amount: u64,
    pub take_profit_price: Option<u64>,
    pub stop_loss_price: Option<u64>,
    pub trigger_price: Option<u64>,
    pub trigger_above_threshold: bool,
    pub bump: u8,
}

// Limit order events - containing ALL fields from msg! calls
#[event]
pub struct LimitOrderExecuted {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub collateral_custody: Pubkey,
    pub position_type: u8,
    pub side: u8,
    pub is_liquidated: bool,
    pub price: u64,
    pub size_usd: u64,
    pub borrow_size_usd: u64,
    pub collateral_usd: u64,
    pub open_time: i64,
    pub update_time: i64,
    pub liquidation_price: u64,
    pub cumulative_interest_snapshot: u128,
    pub opening_fee_paid: u64,
    pub total_fees_paid: u64,
    pub locked_amount: u64,
    pub collateral_amount: u64,
    pub take_profit_price: Option<u64>,
    pub stop_loss_price: Option<u64>,
    pub trigger_price: Option<u64>,
    pub trigger_above_threshold: bool,
    pub bump: u8,
    pub execution_price: u64,
}

#[event]
pub struct LimitOrderCanceled {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub collateral_custody: Pubkey,
    pub position_type: u8,
    pub side: u8,
    pub is_liquidated: bool,
    pub price: u64,
    pub size_usd: u64,
    pub borrow_size_usd: u64,
    pub collateral_usd: u64,
    pub open_time: i64,
    pub update_time: i64,
    pub liquidation_price: u64,
    pub cumulative_interest_snapshot: u128,
    pub opening_fee_paid: u64,
    pub total_fees_paid: u64,
    pub locked_amount: u64,
    pub collateral_amount: u64,
    pub take_profit_price: Option<u64>,
    pub stop_loss_price: Option<u64>,
    pub trigger_price: Option<u64>,
    pub trigger_above_threshold: bool,
    pub bump: u8,
    pub refunded_collateral: u64,
    pub refunded_collateral_usd: u64,
}

// Liquidation events - containing ALL fields from msg! calls
#[event]
pub struct PositionLiquidated {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub collateral_custody: Pubkey,
    pub position_type: u8,
    pub side: u8,
    pub is_liquidated: bool,
    pub price: u64,
    pub size_usd: u64,
    pub borrow_size_usd: u64,
    pub collateral_usd: u64,
    pub open_time: i64,
    pub update_time: i64,
    pub liquidation_price: u64,
    pub cumulative_interest_snapshot: u128,
    pub opening_fee_paid: u64,
    pub total_fees_paid: u64,
    pub locked_amount: u64,
    pub collateral_amount: u64,
    pub take_profit_price: Option<u64>,
    pub stop_loss_price: Option<u64>,
    pub trigger_price: Option<u64>,
    pub trigger_above_threshold: bool,
    pub bump: u8,
    pub settlement_tokens: u64,
    pub liquidator_reward_tokens: u64,
    pub liquidator: Pubkey,
}

// Liquidity events - containing ALL fields from msg! calls
#[event]
pub struct LiquidityAdded {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub amount_in: u64,
    pub deposit_amount: u64,
    pub lp_amount: u64,
    pub fee_amount: u64,
    pub token_amount_usd: u64,
    pub pool_aum_usd: u128,
}

#[event]
pub struct LiquidityRemoved {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub lp_amount_in: u64,
    pub transfer_amount: u64,
    pub fee_amount: u64,
    pub withdrawal_amount: u64,
    pub pool_aum_usd: u128,
}

// Pool management events - containing ALL fields from msg! calls
#[event]
pub struct PoolAdded {
    pub pool: Pubkey,
    pub name: String,
    pub lp_token_mint: Pubkey,
    pub bump: u8,
    pub lp_token_bump: u8,
    pub cumulative_funding_rate_long: i128,
    pub cumulative_funding_rate_short: i128,
    pub cumulative_interest_rate: u128,
    pub long_open_interest_usd: u128,
    pub short_open_interest_usd: u128,
    pub total_borrowed_usd: u128,
}

// TP/SL Orderbook events
#[event]
pub struct TpSlOrderbookInitialized {
    pub owner: Pubkey,
    pub position: Pubkey,
    pub position_type: u8,
    pub bump: u8,
}

#[event]
pub struct TpSlOrderAdded {
    pub owner: Pubkey,
    pub position: Pubkey,
    pub position_type: u8,
    pub order_type: u8, // 0 = TP, 1 = SL
    pub index: u8,
    pub price: u64,
    pub size_percent: u16,
}

#[event]
pub struct TpSlOrderUpdated {
    pub owner: Pubkey,
    pub position: Pubkey,
    pub position_type: u8,
    pub order_type: u8, // 0 = TP, 1 = SL
    pub index: u8,
    pub new_price: Option<u64>,
    pub new_size_percent: Option<u16>,
}

#[event]
pub struct TpSlOrderRemoved {
    pub owner: Pubkey,
    pub position: Pubkey,
    pub position_type: u8,
    pub order_type: u8, // 0 = TP, 1 = SL
    pub index: u8,
}

#[event]
pub struct TpSlOrderExecuted {
    pub owner: Pubkey,
    pub position: Pubkey,
    pub position_type: u8,
    pub order_type: u8, // 0 = TP, 1 = SL
    pub index: u8,
    pub price: u64,
    pub size_percent: u16,
    pub execution_time: i64,
}

// Collateral management events
#[event]
pub struct CollateralAdded {
    pub owner: Pubkey,
    pub position_index: u64,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub collateral_custody: Pubkey,
    pub position_type: u8,
    pub side: u8,
    pub collateral_amount_added: u64,
    pub collateral_usd_added: u64,
    pub new_collateral_amount: u64,
    pub new_collateral_usd: u64,
    pub new_leverage: u64,
    pub new_liquidation_price: u64,
    pub new_borrow_size_usd: u64,
    pub update_time: i64,
}

#[event]
pub struct CollateralRemoved {
    pub owner: Pubkey,
    pub position_index: u64,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub collateral_custody: Pubkey,
    pub position_type: u8,
    pub side: u8,
    pub collateral_amount_removed: u64,
    pub collateral_usd_removed: u64,
    pub new_collateral_amount: u64,
    pub new_collateral_usd: u64,
    pub new_leverage: u64,
    pub new_liquidation_price: u64,
    pub new_borrow_size_usd: u64,
    pub withdrawal_tokens: u64,
    pub withdrawal_asset: Pubkey,
    pub update_time: i64,
}

#[event]
pub struct PositionSizeUpdated {
    pub owner: Pubkey,
    pub position_index: u64,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub collateral_custody: Pubkey,
    pub position_type: u8,
    pub side: u8,
    pub is_increase: bool,
    pub size_delta_usd: u64,
    pub collateral_delta: u64,
    pub previous_size_usd: u64,
    pub new_size_usd: u64,
    pub previous_collateral_usd: u64,
    pub new_collateral_usd: u64,
    pub new_leverage: u64,
    pub new_liquidation_price: u64,
    pub new_borrow_size_usd: u64,
    pub locked_amount_delta: u64,
    pub new_locked_amount: u64,
    pub update_time: i64,
}

#[event]
pub struct BorrowFeesUpdated {
    pub owner: Pubkey,
    pub position_index: u64,
    pub pool: Pubkey,
    pub custody: Pubkey,
    pub collateral_custody: Pubkey,
    pub position_type: u8,
    pub side: u8,
    pub position_size_usd: u64,
    pub borrow_size_usd: u64,
    pub borrow_fee_payment: u64,
    pub new_accrued_borrow_fees: u64,
    pub previous_interest_snapshot: u128,
    pub new_interest_snapshot: u128,
    pub update_time: i64,
}
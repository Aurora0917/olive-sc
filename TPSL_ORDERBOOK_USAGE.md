# TP/SL Orderbook Usage Guide

## Overview
The TP/SL orderbook allows users to set up to 10 take profit and 10 stop loss orders per position, with each order specifying a percentage of the position to close.

## Key Features
- **Multiple Orders**: Up to 10 TP and 10 SL orders per position
- **Percentage-based**: Each order closes a percentage of the position (0-100%)
- **Total Limit**: Combined percentage cannot exceed 100%
- **Backward Compatible**: Existing single TP/SL functionality remains unchanged
- **Optional**: Orderbook is only created when needed

## Usage Flow

### 1. Open Position (Normal Flow)
```rust
// Open perp position as usual - no orderbook created initially
open_perp_position(ctx, params);
```

### 2. Initialize TP/SL Orderbook
```rust
// Create orderbook for advanced TP/SL functionality
let params = InitTpSlOrderbookParams {
    position_type: 0,  // 0 = Perp, 1 = Option
    position_index: 1, // Your position index
    pool_name: "SOL-USDC".to_string(),
};
init_tp_sl_orderbook(ctx, params);
```

### 3. Add TP/SL Orders
```rust
// Add 25% take profit at $200
let params = ManageTpSlOrdersParams {
    position_type: 0,
    position_index: 1,
    pool_name: "SOL-USDC".to_string(),
    action: OrderAction::AddTakeProfit {
        price: 200_000_000, // $200 (scaled by 1e6)
        size_percent: 2500,  // 25% (in basis points)
    },
};
manage_tp_sl_orders(ctx, params);

// Add 50% take profit at $250
let params = ManageTpSlOrdersParams {
    position_type: 0,
    position_index: 1,
    pool_name: "SOL-USDC".to_string(),
    action: OrderAction::AddTakeProfit {
        price: 250_000_000, // $250
        size_percent: 5000,  // 50%
    },
};
manage_tp_sl_orders(ctx, params);

// Add 20% stop loss at $180
let params = ManageTpSlOrdersParams {
    position_type: 0,
    position_index: 1,
    pool_name: "SOL-USDC".to_string(),
    action: OrderAction::AddStopLoss {
        price: 180_000_000, // $180
        size_percent: 2000,  // 20%
    },
};
manage_tp_sl_orders(ctx, params);
```

### 4. Update Existing Orders
```rust
// Update the first TP order to $220
let params = ManageTpSlOrdersParams {
    position_type: 0,
    position_index: 1,
    pool_name: "SOL-USDC".to_string(),
    action: OrderAction::UpdateTakeProfit {
        index: 0,  // First TP order
        new_price: Some(220_000_000), // New price
        new_size_percent: None,        // Keep same percentage
    },
};
manage_tp_sl_orders(ctx, params);
```

### 5. Remove Orders
```rust
// Remove the second TP order
let params = ManageTpSlOrdersParams {
    position_type: 0,
    position_index: 1,
    pool_name: "SOL-USDC".to_string(),
    action: OrderAction::RemoveTakeProfit { index: 1 },
};
manage_tp_sl_orders(ctx, params);
```

### 6. Clear All Orders
```rust
// Remove all TP and SL orders
let params = ManageTpSlOrdersParams {
    position_type: 0,
    position_index: 1,
    pool_name: "SOL-USDC".to_string(),
    action: OrderAction::ClearAll,
};
manage_tp_sl_orders(ctx, params);
```

## Price Scaling
- All prices are scaled by 1e6 (6 decimal places)
- Example: $200.50 = 200_500_000

## Size Percentage
- Expressed in basis points (1 basis point = 0.01%)
- Example: 25% = 2500 basis points
- Maximum total: 10000 basis points (100%)

## Validation Rules

### For Perpetual Positions
- **Take Profit**: Must be above entry price for longs, below for shorts
- **Stop Loss**: Must be below entry price for longs, above for shorts
- **Stop Loss**: Cannot be beyond liquidation price

### For Options
- **Call Options**: TP > strike price, SL < strike price
- **Put Options**: TP < strike price, SL > strike price

## Account Structure

### Seeds for TpSlOrderbook
```
[
    b"tp_sl_orderbook",
    owner.key().as_ref(),
    position_index.to_le_bytes().as_ref(),
    pool_name.as_bytes(),
    position_type.to_le_bytes().as_ref(),
]
```

## Events
The system emits events for all orderbook operations:
- `TpSlOrderbookInitialized`
- `TpSlOrderAdded`
- `TpSlOrderUpdated`
- `TpSlOrderRemoved`
- `TpSlOrderExecuted`

## Backend Integration
The backend should monitor orderbook accounts and execute orders when price conditions are met using the existing position closing mechanisms.
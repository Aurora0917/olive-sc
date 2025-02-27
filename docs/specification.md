## Add Custody

Steps:
- Verify if signer is one of multisigs
- Terminate early if not all multisigs signers are signed yet
- Check that there is no other custody for this token
- Validate pool
- Update pool account
- Update custody account


## Add Liquidity


## Make Option

### Inputs:
- Token Custody (For eg, It would be BTC if Buy BTC call opt)
- Oracle account of custody token (BTC)
- Option pda should be made in:
    - "option" + user.pubkey + pool.pubkey + custody.pubkey + "buy" | "sell"

### Steps:
- Assert permission:
    - Ensure that custody provided is not stable
- Retrieve oracle token price of custody (See #Price retrieval Strategy) and select min price
- Compute premium of option given token price and buyer's option input
- Calculate fee of trade (See Fee computation)
- Total transfer amount = Option Premium + fee (in USD)
- Create Option account
- Transfer WBTC from user to custody account as locked liquidity
    - Check that it satisfies utilization
- Update Custody stats changed from this trade (for record keeping only)
    - Collected fees
    - volume stats
    - collateral
    - protocol fees 
- Update hourly borrow rate of custody asset (See update borrow rate)


# Pool validation

Invariant rules:
- All token ratios must be validated:
    - all min, max and target must be < 10000
- Only one token per custody
- Target ratios sum to 1


# Price retrieval strategy


# Option premium computation


# Fee computation
```
total_fee = custody_fee_pct * option_size (maybe use delta) * utilization_fee

where

utilization_fee = ultilization_mult * (new_utilization - optimal) / (1 - optimal)
```

Greater the deviation from optimal, higher the fees incurred

Henry comment: I don't know why denominator is (1 - optimal)

# Borrow Rate update
```
if current_utilization < optimal_utilization:
    rate = base_rate + (current_utilization / optimal_utilization) * slope1
else:
    rate = base_rate + slope1 + (current_utilization - optimal_utilization) / (1 - optimal_utilization) * slope2
```

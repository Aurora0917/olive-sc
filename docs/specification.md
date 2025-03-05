## Add Custody

Steps:
- Verify if signer is one of multisigs
- Terminate early if not all multisigs signers are signed yet
- Check that there is no other custody for this token
- Validate pool
- Update pool account
- Update custody account


## Add Liquidity

Add liquidity to pool

Steps:
- Assert permissions
- Validate inputs
- Retrieve oracle token price
- Compute add liquidity fee 
- deposit_amt <- amount_in - fee
- Check token ratio
- Transfer token from user to custody account
- Compute Pool AUM in USD
- Compute how much LP token user should receive:
    - token_amt_in_usd <- min(spot, ewma_price) * (amount_in - fee)
    - lp_amount <- token_amt_in_usd / pool_aum_usd * lp_token_supply
- Mint LP token to user's account
- Update custody stats:
    - Collected fees
    - protocol fees
    - increase owned amount
- Update custody borrow rate
- Update pool's AUM number

## Exercise Option

## Steps:
- Assert that option position is not alrdy exercised
- Retrieve oracle token price of custody
- if call option:
    - if token_price >= strike_price:
        - profit <- (token_price_in_usd - token_strike_price_in_usd) * num_of_contracts
- if call option:
    - if token_price <= strike_price:
        - profit <- (strike_price - token_price_in_usd) * num_of_contracts
- reward_amt <- reward_fee * profit
- user_amt <- profit - reward_amt - fee
- Update custody locked amount by reducing transfer amt (Unlocking)
- Transfer user_amt from custody wallet to user receiving wallet
- Transfer reward amt from custody wallet to signer
- Update custody stats:
    - Collected fee
    - assets_owned


## Purchase Option

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
- Total transfer amount (WBTC) = Option Premium + fee (in USD)
- Create Option account
- Transfer WBTC from user to custody account as locked liquidity
    - Check that it satisfies utilization
- Update locked amount of WBTC in custody by transfer amount
- Update Custody stats changed from this trade (for record keeping only)
    - Collected fees
    - volume stats
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

Using Black-Scholes equation

# Fee computation
## Generic Fee

Applies to:
- AddLiquidity

```
if token_ratio is improved:
    fee <- base_fee / ratio_fee
otherwise
    fee <- base_fee * ratio_fee

where:
    if new_ratio < target:
        ratio_fee <- 1 + custody.fees.ratio_mult * (target - new_ratio) / (target - min)
    else:
        ratio_fee <- 1 + custody.fees.ratio_mult * (new_ratio - target) / (max - target)
```

## Buy Fee
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
# Check Token Ratio

Ensure that updated token ratio is moved within  min, max specified

if new_ratio < min:
    assert that new_ratio >= current_ratio
else if new_ratio > max:
    assert that new_ratio <= current_ratio


# Pool AUM computation

```
AUM <- sum(token_owned + token_price) + interest_earned
```
use crate::{
    errors::{FutureError, TradingError},
    events::FutureOpened,
    math::{self, f64_to_scaled_price},
    state::{Contract, Custody, Future, FutureStatus, OraclePrice, Pool, Side, User},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct OpenFutureParams {
    pub side: Side,                    // Long or Short
    pub size_usd: u64,                // Position size in USD (6 decimals)
    pub collateral_amount: u64,       // Collateral tokens to deposit
    pub pay_sol: bool,                // Pay collateral in SOL or USDC
    pub expiry_timestamp: i64,        // Future expiry time (unix timestamp)
    pub max_slippage_bps: u64,        // Maximum slippage tolerance in basis points
    pub pool_name: String,            // Pool name for seeds
}

pub fn open_future(ctx: Context<OpenFuture>, params: &OpenFutureParams) -> Result<()> {
    msg!("Opening future position");
    msg!("Side: {:?}", params.side);
    msg!("Size USD: {}", params.size_usd);
    msg!("Collateral amount: {}", params.collateral_amount);
    msg!("Expiry: {}", params.expiry_timestamp);

    // Get keys first to avoid borrowing conflicts
    let sol_custody_key = ctx.accounts.sol_custody.key();
    let usdc_custody_key = ctx.accounts.usdc_custody.key();
    
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let future = &mut ctx.accounts.future;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;

    // Get current time and validate expiry
    let current_time = contract.get_time()?;
    
    require!(
        params.expiry_timestamp > current_time,
        FutureError::InvalidExpiryTime
    );
    
    // Validate expiry is not too far in the future (max 1 year)
    let max_expiry = current_time + (365 * 24 * 3600);
    require!(
        params.expiry_timestamp <= max_expiry,
        FutureError::ExpiryTooFar
    );
    
    // Validate expiry is not too close (min 1 hour)
    let min_expiry = current_time + 3600;
    require!(
        params.expiry_timestamp >= min_expiry,
        FutureError::ExpiryTooClose
    );

    let time_to_expiry = params.expiry_timestamp - current_time;

    // Validate position size
    require!(params.size_usd > 0, FutureError::InvalidFutureSize);
    require!(
        params.size_usd >= 1_000_000, // Minimum $1
        FutureError::FutureSizeTooSmall
    );
    require!(
        params.size_usd <= 1_000_000_000_000, // Maximum $1M
        FutureError::FutureSizeTooLarge
    );

    // Get oracle prices
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let current_sol_price = sol_price.get_price();
    let current_sol_price_scaled = f64_to_scaled_price(current_sol_price)?;

    let current_usdc_price = usdc_price.get_price();
    let current_usdc_price_scaled = f64_to_scaled_price(current_usdc_price)?;

    // Calculate fees
    let opening_fee = math::checked_div(
        math::checked_mul(params.size_usd as u128, Future::OPENING_FEE_BPS as u128)?,
        10_000u128,
    )? as u64;

    let settlement_fee = math::checked_div(
        math::checked_mul(params.size_usd as u128, Future::SETTLEMENT_FEE_BPS as u128)?,
        10_000u128,
    )? as u64;

    // Calculate collateral value in USD
    let collateral_usd = if params.pay_sol {
        // Convert SOL to USD
        let sol_amount_usd = math::checked_div(
            math::checked_mul(params.collateral_amount as u128, current_sol_price_scaled as u128)?,
            math::checked_pow(10u128, sol_custody.decimals as usize)? // Convert from token decimals to base
        )? as u64;
        sol_amount_usd
    } else {
        // Convert USDC to USD (USDC is pegged to $1)
        let usdc_amount_usd = math::checked_div(
            math::checked_mul(params.collateral_amount as u128, current_usdc_price_scaled as u128)?, // $1 = 1_000_000 (6 decimals)
            math::checked_pow(10u128, usdc_custody.decimals as usize)?
        )? as u64;
        usdc_amount_usd
    } - opening_fee;

    msg!("Collateral USD value: {}", collateral_usd);

    // Validate leverage
    let leverage = math::checked_div(params.size_usd as u128, collateral_usd as u128)? as f64;
    require!(
        leverage <= Future::MAX_LEVERAGE,
        FutureError::MaxFutureLeverageExceeded
    );
    require!(
        collateral_usd > 0,
        FutureError::InsufficientCollateralForFuture
    );

    // Calculate and lock fixed interest rate using 2D utilization
    let fixed_rate_bps = pool.add_future_position(
        params.size_usd,
        time_to_expiry,
        current_time,
    )?;

    msg!("Fixed rate locked: {}bps", fixed_rate_bps);

    // Calculate theoretical future price: F = S * exp(r * T)
    let time_to_expiry_years = (time_to_expiry as f64) / (365.0 * 24.0 * 3600.0);
    let theoretical_future_price = Future::calculate_theoretical_price(
        current_sol_price,
        fixed_rate_bps,
        time_to_expiry_years,
    )?;
    let future_price_scaled = f64_to_scaled_price(theoretical_future_price)?;

    msg!("Spot price: {}", current_sol_price);
    msg!("Future price: {}", theoretical_future_price);
    msg!("Time to expiry (days): {}", time_to_expiry / (24 * 3600));

    // We'll calculate liquidation price after initializing future struct
    // since the new formula needs the future's properties

    // Calculate locked amount (for pool liquidity)
    let locked_amount = if params.side == Side::Long {
        // Long positions lock underlying asset (SOL)
        let sol_price_scaled = sol_price.scale_to_exponent(-6)?;
        let sol_amount_6_decimals = math::checked_div(
            math::checked_mul(params.size_usd as u128, 1_000_000u128)?,
            sol_price_scaled.price as u128
        )?;
        
        if sol_custody.decimals > 6 {
            math::checked_as_u64(math::checked_mul(
                sol_amount_6_decimals,
                math::checked_pow(10u128, (sol_custody.decimals - 6) as usize)?
            )?)?
        } else {
            math::checked_as_u64(math::checked_div(
                sol_amount_6_decimals,
                math::checked_pow(10u128, (6 - sol_custody.decimals) as usize)?
            )?)?
        }
    } else {
        // Short positions lock stable coin (USDC)
        let usdc_price_scaled = usdc_price.scale_to_exponent(-6)?;
        let usdc_amount_6_decimals = math::checked_div(
            math::checked_mul(params.size_usd as u128, 1_000_000u128)?,
            usdc_price_scaled.price as u128
        )?;
        
        if usdc_custody.decimals > 6 {
            math::checked_as_u64(math::checked_mul(
                usdc_amount_6_decimals,
                math::checked_pow(10u128, (usdc_custody.decimals - 6) as usize)?
            )?)?
        } else {
            math::checked_as_u64(math::checked_div(
                usdc_amount_6_decimals,
                math::checked_pow(10u128, (6 - usdc_custody.decimals) as usize)?
            )?)?
        }
    };

    // Check pool has sufficient liquidity
    let available_liquidity = if params.side == Side::Long {
        math::checked_sub(sol_custody.token_owned, sol_custody.token_locked)?
    } else {
        math::checked_sub(usdc_custody.token_owned, usdc_custody.token_locked)?
    };
    
    require!(
        available_liquidity >= locked_amount,
        TradingError::InsufficientPoolLiquidity
    );
    let collateral_token_account = if params.pay_sol {
        &ctx.accounts.sol_custody_token_account
    } else {
        &ctx.accounts.usdc_custody_token_account
    };

    contract.transfer_tokens(
        ctx.accounts.funding_account.to_account_info(),
        collateral_token_account.to_account_info(),
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        params.collateral_amount,
    )?;

    // Update custody balances
    if params.pay_sol {
        sol_custody.token_owned = math::checked_add(
            sol_custody.token_owned,
            params.collateral_amount
        )?;
    } else {
        usdc_custody.token_owned = math::checked_add(
            usdc_custody.token_owned,
            params.collateral_amount
        )?;
    }

    // Lock liquidity in relevant custody
    if params.side == Side::Long {
        sol_custody.token_locked = math::checked_add(
            sol_custody.token_locked,
            locked_amount
        )?;
    } else {
        usdc_custody.token_locked = math::checked_add(
            usdc_custody.token_locked,
            locked_amount
        )?;
    }

    // Initialize future position  
    future.index = ctx.accounts.user.future_index;
    
    // Increment user's future index for next future
    ctx.accounts.user.future_index = math::checked_add(ctx.accounts.user.future_index, 1)?;
    future.owner = ctx.accounts.owner.key();
    future.pool = pool.key();
    future.custody = sol_custody_key; // Always SOL as underlying
    future.collateral_custody = if params.pay_sol {
        sol_custody_key
    } else {
        usdc_custody_key
    };
    
    future.side = params.side;
    future.status = FutureStatus::Active;
    
    future.entry_price = current_sol_price_scaled;
    future.future_price = future_price_scaled;
    future.size_usd = params.size_usd;
    future.collateral_usd = collateral_usd;
    future.collateral_amount = params.collateral_amount;
    
    future.open_time = current_time;
    future.expiry_time = params.expiry_timestamp;
    future.update_time = current_time;
    future.settlement_time = None;
    
    future.fixed_interest_rate_bps = fixed_rate_bps;
    future.time_to_expiry_at_open = time_to_expiry;
    
    // Calculate liquidation price using the new formula
    future.liquidation_price = future.calculate_liquidation_price(current_time)?;
    future.maintenance_margin_bps = Future::MAINTENANCE_MARGIN_BPS;
    
    future.settlement_price = None;
    future.pnl_at_settlement = None;
    future.settlement_amount = None;
    
    future.opening_fee = opening_fee;
    future.settlement_fee = settlement_fee;
    
    future.locked_amount = locked_amount;
    future.bump = ctx.bumps.future;

    emit!(FutureOpened {
        owner: future.owner,
        future_key: future.key(),
        index: future.index,
        pool: pool.key(),
        custody: sol_custody_key,
        collateral_custody: future.collateral_custody,
        side: future.side as u8,
        size_usd: params.size_usd,
        collateral_usd,
        collateral_amount: params.collateral_amount,
        entry_price: current_sol_price_scaled,
        future_price: future_price_scaled,
        fixed_interest_rate_bps: fixed_rate_bps,
        expiry_time: params.expiry_timestamp,
        liquidation_price: future.liquidation_price,
        locked_amount,
        open_time: current_time,
    });

    msg!("Future position opened successfully");
    msg!("Future price: {}", future_price_scaled);
    msg!("Liquidation price: {}", future.liquidation_price);

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: OpenFutureParams)]
pub struct OpenFuture<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        init_if_needed,
        payer = owner,
        space = User::LEN,
        seeds = [b"user_v3", owner.key().as_ref()],
        bump
    )]
    pub user: Box<Account<'info, User>>,

    #[account(
        mut,
        has_one = owner
    )]
    pub funding_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: Transfer authority PDA for contract token operations
    #[account(
        seeds = [b"transfer_authority"],
        bump = contract.transfer_authority_bump
    )]
    pub transfer_authority: AccountInfo<'info>,

    #[account(
        seeds = [b"contract"],
        bump = contract.bump
    )]
    pub contract: Box<Account<'info, Contract>>,

    #[account(
        mut,
        seeds = [b"pool", params.pool_name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        init,
        payer = owner,
        space = Future::LEN,
        seeds = [
            b"future",
            owner.key().as_ref(),
            user.future_index.to_le_bytes().as_ref(),
            pool.key().as_ref()
        ],
        bump
    )]
    pub future: Box<Account<'info, Future>>,

    #[account(
        mut,
        seeds = [b"custody", pool.key().as_ref(), sol_mint.key().as_ref()],
        bump = sol_custody.bump
    )]
    pub sol_custody: Box<Account<'info, Custody>>,

    #[account(
        mut,
        seeds = [b"custody", pool.key().as_ref(), usdc_mint.key().as_ref()],
        bump = usdc_custody.bump
    )]
    pub usdc_custody: Box<Account<'info, Custody>>,

    /// CHECK: Oracle account validation is handled by constraint
    #[account(
        constraint = sol_oracle_account.key() == sol_custody.oracle
    )]
    pub sol_oracle_account: AccountInfo<'info>,

    /// CHECK: Oracle account validation is handled by constraint
    #[account(
        constraint = usdc_oracle_account.key() == usdc_custody.oracle
    )]
    pub usdc_oracle_account: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [
            b"custody_token_account",
            pool.key().as_ref(),
            sol_custody.mint.key().as_ref()
        ],
        bump
    )]
    pub sol_custody_token_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [
            b"custody_token_account",
            pool.key().as_ref(),
            usdc_custody.mint.key().as_ref()
        ],
        bump
    )]
    pub usdc_custody_token_account: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub sol_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub usdc_mint: Box<Account<'info, Mint>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
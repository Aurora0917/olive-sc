use crate::{
    errors::{PerpetualError, PoolError, TradingError},
    events::PerpPositionOpened,
    math::{self, f64_to_scaled_price},
    state::{Contract, Custody, OraclePrice, OrderType, Pool, Position, Side, User},
    utils::risk_management::*,
};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct OpenPerpPositionParams {
    pub size_amount: u64,              // Position amount in tokens
    pub collateral_amount: u64,        // Collateral amount in tokens
    pub side: Side,                    // Long or Short
    pub order_type: OrderType,         // Market or Limit
    pub trigger_price: Option<u64>,    // For limit orders
    pub trigger_above_threshold: bool, // Direction for limit orders
    pub max_slippage: u64,             // Max acceptable slippage in basis points
    pub pool_name: String,             // Pool name
    pub pay_sol: bool,                 // true = pay with SOL, false = pay with USDC
}

pub fn open_perp_position(
    ctx: Context<OpenPerpPosition>,
    params: &OpenPerpPositionParams,
) -> Result<()> {
    msg!("Opening perpetual position");

    let owner = &ctx.accounts.owner;
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    let position = &mut ctx.accounts.position;
    let user: &mut Box<Account<'_, User>> = &mut ctx.accounts.user;

    // Basic validation
    require!(params.size_amount > 0, TradingError::InvalidAmount);
    require!(params.collateral_amount > 0, TradingError::InvalidAmount);
    require!(params.max_slippage <= 1000, TradingError::InvalidSlippage); // Max 10%
    require!(!params.pool_name.is_empty(), PoolError::InvalidPoolName);

    // Get current prices
    let current_time = contract.get_time()?;
    let sol_price =
        OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price =
        OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;

    let sol_price_value = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();

    msg!("SOL Price: {}", sol_price_value);
    msg!("USDC Price: {}", usdc_price_value);

    // Determine collateral asset and custody
    let (collateral_custody, collateral_decimals, collateral_price) = if params.pay_sol {
        (sol_custody.key(), sol_custody.decimals, sol_price_value)
    } else {
        (usdc_custody.key(), usdc_custody.decimals, usdc_price_value)
    };

    // Calculate collateral value in USD
    let collateral_usd = math::checked_as_u64(
        math::checked_float_mul(
            params.collateral_amount as f64 / math::checked_powi(10.0, collateral_decimals as i32)?,
            collateral_price,
        )? * 1_000_000.0,
    )?;

    let size_usd = math::checked_as_u64(
        math::checked_float_mul(
            params.size_amount as f64 / math::checked_powi(10.0, collateral_decimals as i32)?,
            collateral_price,
        )? * 1_000_000.0,
    )?;

    // Calculate leverage
    let leverage =
        math::checked_float_div(params.size_amount as f64, params.collateral_amount as f64)?;

    msg!("Position Size USD: {}", size_usd);
    msg!("Collateral USD: {}", collateral_usd);
    msg!("Leverage: {}x", leverage);

    // Validate leverage (100x max)
    require!(
        leverage <= Position::MAX_LEVERAGE && leverage >= 1.0,
        PerpetualError::InvalidLeverage
    );

    // Check user has sufficient balance
    require_gte!(
        ctx.accounts.funding_account.amount,
        params.collateral_amount,
        TradingError::InsufficientBalance
    );

    // Calculate margin requirements
    let initial_margin_bps = math::checked_as_u64(math::checked_float_div(10_000.0, leverage)?)?; // 10000 / leverage

    // Ensure minimum margin requirements
    require!(
        initial_margin_bps >= Position::MIN_INITIAL_MARGIN_BPS,
        PerpetualError::InvalidLeverage
    );

    // Calculate liquidation price
    let entry_price = if params.order_type == OrderType::Limit {
        params
            .trigger_price
            .unwrap_or(f64_to_scaled_price(sol_price_value)?)
    } else {
        f64_to_scaled_price(sol_price_value)?
    };

    let liquidation_price = calculate_liquidation_price(entry_price, leverage, params.side)?;

    msg!("Entry Price: {}", entry_price);
    msg!("Liquidation Price: {}", liquidation_price);

    // Check pool liquidity using integer math
    let required_liquidity = if params.side == Side::Long {
        // Convert USD to SOL tokens using integer math
        let sol_price_scaled = sol_price.scale_to_exponent(-6)?;
        let sol_amount_6_decimals = math::checked_div(
            math::checked_mul(size_usd as u128, 1_000_000u128)?,
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
        // Convert USD to USDC tokens using integer math
        let usdc_price_scaled = usdc_price.scale_to_exponent(-6)?;
        let usdc_amount_6_decimals = math::checked_div(
            math::checked_mul(size_usd as u128, 1_000_000u128)?,
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

    let normalized_collateral_amount = if params.side == Side::Long {
        // For long positions, convert collateral to SOL token units
        if params.pay_sol {
            // Already in SOL, use as-is
            params.collateral_amount
        } else {
            // Convert USDC collateral to equivalent SOL tokens using integer math
            let sol_price_scaled = sol_price.scale_to_exponent(-6)?;
            let sol_amount_6_decimals = math::checked_div(
                math::checked_mul(collateral_usd as u128, 1_000_000u128)?,
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
        }
    } else {
        // For short positions, convert collateral to USDC token units
        if params.pay_sol {
            // Convert SOL collateral to equivalent USDC tokens using integer math
            let usdc_price_scaled = usdc_price.scale_to_exponent(-6)?;
            let usdc_amount_6_decimals = math::checked_div(
                math::checked_mul(collateral_usd as u128, 1_000_000u128)?,
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
        } else {
            // Already in USDC, use as-is
            params.collateral_amount
        }
    };

    if params.side == Side::Long {
        require_gte!(
            sol_custody.token_owned,
            required_liquidity,
            TradingError::InsufficientPoolLiquidity
        );
    } else {
        require_gte!(
            usdc_custody.token_owned,
            required_liquidity,
            TradingError::InsufficientPoolLiquidity
        );
    }

    // Transfer collateral from user to pool
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            SplTransfer {
                from: ctx.accounts.funding_account.to_account_info(),
                to: if params.pay_sol {
                    ctx.accounts.sol_custody_token_account.to_account_info()
                } else {
                    ctx.accounts.usdc_custody_token_account.to_account_info()
                },
                authority: owner.to_account_info(),
            },
        ),
        params.collateral_amount,
    )?;

    // Update custody stats - lock tokens only for MARKET orders (limit orders lock when executed)
    if params.order_type == OrderType::Market {
        if params.side == Side::Long {
            // Long positions always need SOL backing
            sol_custody.token_locked =
                math::checked_add(sol_custody.token_locked, required_liquidity)?;
        } else {
            // Short positions always need USDC backing
            usdc_custody.token_locked =
                math::checked_add(usdc_custody.token_locked, required_liquidity)?;
        }
    }

    if params.pay_sol {
        sol_custody.token_owned =
            math::checked_add(sol_custody.token_owned, params.collateral_amount)?;
    } else {
        usdc_custody.token_owned =
            math::checked_add(usdc_custody.token_owned, params.collateral_amount)?;
    }

    // Initialize position
    position.index = user.perp_position_count.checked_add(1).unwrap_or(1);
    position.owner = owner.key();
    position.pool = pool.key();
    position.custody = sol_custody.key(); // Position always tracks SOL
    position.collateral_custody = collateral_custody;
    position.order_type = params.order_type;
    position.side = params.side;
    position.is_liquidated = false;
    position.price = entry_price;
    position.size_usd = size_usd;
    position.collateral_usd = collateral_usd;
    position.open_time = current_time;
    position.update_time = current_time;
    position.execution_time = if params.order_type == OrderType::Market {
        Some(current_time) // Market orders execute immediately
    } else {
        None // Limit orders start with no execution time
    };
    position.last_borrow_fees_update_time = current_time;
    position.liquidation_price = liquidation_price;

    // Set snapshots from current pool state (side-specific)
    position.cumulative_interest_snapshot = match params.side {
        Side::Long => pool.cumulative_interest_rate_long,
        Side::Short => pool.cumulative_interest_rate_short,
    };

    position.trade_fees = math::checked_div(
        math::checked_mul(size_usd, Position::EXITING_FEE_BPS)?,
        10000,
    )?;
    position.borrow_fees_paid = 0;

    position.accrued_borrow_fees = 0;

    // Asset amounts
    position.locked_amount = required_liquidity;
    position.collateral_amount = normalized_collateral_amount;

    // TP/SL
    position.tp_sl_orderbook = None; // No orderbook initially

    // Limit order specific
    position.trigger_price = params.trigger_price;
    position.trigger_above_threshold = params.trigger_above_threshold;

    position.bump = ctx.bumps.position;

    // Update pool open interest
    if params.side == Side::Long {
        pool.long_open_interest_usd =
            math::checked_add(pool.long_open_interest_usd, size_usd as u128)?;
    } else {
        pool.short_open_interest_usd =
            math::checked_add(pool.short_open_interest_usd, size_usd as u128)?;
    }

    // Update user stats
    user.perp_position_count = user.perp_position_count.checked_add(1).unwrap_or(1);

    emit!(PerpPositionOpened {
        index: position.index,
        owner: position.owner,
        pool: position.pool,
        pub_key: position.key(),
        custody: position.custody,
        collateral_custody: position.collateral_custody,
        order_type: position.order_type as u8,
        side: position.side as u8,
        is_liquidated: position.is_liquidated,
        price: position.price,
        size_usd: position.size_usd,
        collateral_usd: position.collateral_usd,
        open_time: position.open_time,
        execution_time: position.execution_time,
        update_time: position.update_time,
        last_borrow_fees_update_time: position.last_borrow_fees_update_time,
        liquidation_price: position.liquidation_price,
        cumulative_interest_snapshot: position.cumulative_interest_snapshot,
        trade_fees: position.trade_fees,
        accrued_borrow_fees: position.accrued_borrow_fees,
        locked_amount: position.locked_amount,
        collateral_amount: position.collateral_amount,
        trigger_price: position.trigger_price,
        trigger_above_threshold: position.trigger_above_threshold,
        max_slippage: params.max_slippage,
        bump: position.bump,
    });

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: OpenPerpPositionParams)]
pub struct OpenPerpPosition<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = owner
    )]
    pub funding_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: Program derived address (PDA) used as authority for token operations.
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
        init_if_needed,
        payer = owner,
        space = User::LEN,
        seeds = [b"user", owner.key().as_ref()],
        bump,
    )]
    pub user: Box<Account<'info, User>>,

    #[account(
        init,
        payer = owner,
        space = Position::LEN,
        seeds = [
            b"position",
            owner.key().as_ref(),
            (user.perp_position_count + 1).to_le_bytes().as_ref(),
            pool.key().as_ref()
        ],
        bump
    )]
    pub position: Box<Account<'info, Position>>,

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

    #[account(mut)]
    pub sol_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub usdc_mint: Box<Account<'info, Mint>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

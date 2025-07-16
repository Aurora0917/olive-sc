use crate::{
    errors::{PerpetualError, TradingError, PoolError},
    math::{self, f64_to_scaled_price},
    state::{Contract, Custody, OraclePrice, Pool, User, Position, Side, PositionType},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct OpenPerpPositionParams {
    pub size_usd: u64,              // Position size in USD
    pub collateral_amount: u64,     // Collateral amount in tokens
    pub side: Side,                 // Long or Short
    pub position_type: PositionType, // Market or Limit
    pub trigger_price: Option<u64>, // For limit orders
    pub trigger_above_threshold: bool, // Direction for limit orders
    pub max_slippage: u64,          // Max acceptable slippage in basis points
    pub pool_name: String,          // Pool name
    pub pay_sol: bool,              // true = pay with SOL, false = pay with USDC
    pub take_profit_price: Option<u64>, // Optional TP price
    pub stop_loss_price: Option<u64>,   // Optional SL price
}

pub fn open_perp_position(
    ctx: Context<OpenPerpPosition>, 
    params: &OpenPerpPositionParams
) -> Result<()> {
    msg!("Opening perpetual position");
    
    let owner = &ctx.accounts.owner;
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    let position = &mut ctx.accounts.position;
    let user = &mut ctx.accounts.user;
    
    // Basic validation
    require!(params.size_usd > 0, TradingError::InvalidAmount);
    require!(params.collateral_amount > 0, TradingError::InvalidAmount);
    require!(params.max_slippage <= 1000, TradingError::InvalidSlippage); // Max 10%
    require!(!params.pool_name.is_empty(), PoolError::InvalidPoolName);
    
    // Get current prices
    let current_time = contract.get_time()?;
    let sol_price = OraclePrice::new_from_oracle(
        &ctx.accounts.sol_oracle_account, 
        current_time, 
        false
    )?;
    let usdc_price = OraclePrice::new_from_oracle(
        &ctx.accounts.usdc_oracle_account, 
        current_time, 
        false
    )?;
    
    let sol_price_value = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();
    
    msg!("SOL Price: {}", sol_price_value);
    msg!("USDC Price: {}", usdc_price_value);
    
    // Determine collateral asset and custody
    let (collateral_custody, collateral_decimals, collateral_price) = 
        if params.pay_sol {
            (sol_custody.key(), sol_custody.decimals, sol_price_value)
        } else {
            (usdc_custody.key(), usdc_custody.decimals, usdc_price_value)
        };
    
    // Calculate collateral value in USD
    let collateral_usd = math::checked_as_u64(math::checked_float_mul(
        params.collateral_amount as f64 / math::checked_powi(10.0, collateral_decimals as i32)?,
        collateral_price
    )?)?;
    
    // Calculate leverage
    let leverage = math::checked_div(params.size_usd, collateral_usd)?;
    
    msg!("Position Size USD: {}", params.size_usd);
    msg!("Collateral USD: {}", collateral_usd);
    msg!("Leverage: {}x", leverage);
    
    // Update pool rates using the borrow rate curve
    let current_time = Clock::get()?.unix_timestamp;
    
    // Get custody accounts for utilization calculation  
    let custodies_slice = [sol_custody.as_ref(), usdc_custody.as_ref()];
    let custodies_vec: Vec<Custody> = custodies_slice.iter().map(|c| (***c).clone()).collect();
    pool.update_rates(current_time, &custodies_vec)?;
    
    // Validate leverage (250x max)
    require!(
        leverage <= Position::MAX_LEVERAGE && leverage >= 1,
        PerpetualError::InvalidLeverage
    );
    
    // Check user has sufficient balance
    require_gte!(
        ctx.accounts.funding_account.amount,
        params.collateral_amount,
        TradingError::InsufficientBalance
    );
    
    // Calculate margin requirements
    let initial_margin_bps = math::checked_div(10_000u64, leverage)?; // 10000 / leverage
    let maintenance_margin_bps = math::checked_div(initial_margin_bps, 2)?; // Half of initial
    
    // Ensure minimum margin requirements
    require!(
        initial_margin_bps >= Position::MIN_INITIAL_MARGIN_BPS,
        PerpetualError::InvalidLeverage
    );
    
    // Calculate liquidation price
    let entry_price = if params.position_type == PositionType::Limit {
        params.trigger_price.unwrap_or(f64_to_scaled_price(sol_price_value)?)
    } else {
        f64_to_scaled_price(sol_price_value)?
    };
    
    let liquidation_price = calculate_liquidation_price(
        entry_price,
        maintenance_margin_bps,
        params.side
    )?;
    
    msg!("Entry Price: {}", entry_price);
    msg!("Liquidation Price: {}", liquidation_price);
    
    // Check pool liquidity
    let required_liquidity = if params.side == Side::Long {
        // For longs, need SOL backing
        math::checked_as_u64(params.size_usd as f64 / sol_price_value)?
    } else {
        // For shorts, need USDC backing
        params.size_usd
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
    
    // Update custody stats
    if params.pay_sol {
        sol_custody.token_owned = math::checked_add(
            sol_custody.token_owned,
            params.collateral_amount
        )?;
        
        if params.side == Side::Long {
            sol_custody.token_locked = math::checked_add(
                sol_custody.token_locked,
                required_liquidity
            )?;
        }
    } else {
        usdc_custody.token_owned = math::checked_add(
            usdc_custody.token_owned,
            params.collateral_amount
        )?;
        
        if params.side == Side::Short {
            usdc_custody.token_locked = math::checked_add(
                usdc_custody.token_locked,
                required_liquidity
            )?;
        }
    }
    
    // Initialize position
    position.owner = owner.key();
    position.pool = pool.key();
    position.custody = sol_custody.key(); // Position always tracks SOL
    position.collateral_custody = collateral_custody;
    position.position_type = params.position_type;
    position.side = params.side;
    position.is_liquidated = false;
    position.price = entry_price;
    position.size_usd = params.size_usd;
    position.borrow_size_usd = params.size_usd - collateral_usd; // Borrowed amount
    position.collateral_usd = collateral_usd;
    position.open_time = current_time;
    position.update_time = current_time;
    position.liquidation_price = liquidation_price;
    position.initial_margin_bps = initial_margin_bps;
    position.maintenance_margin_bps = maintenance_margin_bps;
    
    // Set snapshots from current pool state
    position.cumulative_interest_snapshot = pool.cumulative_interest_rate;
    position.cumulative_funding_snapshot = if position.side == Side::Long {
        pool.cumulative_funding_rate_long.try_into().unwrap()
    } else {
        pool.cumulative_funding_rate_short.try_into().unwrap()
    };
    
    // Fee tracking
    let opening_fee = math::checked_div(params.size_usd, 1000)?; // 0.1% opening fee
    position.opening_fee_paid = opening_fee;
    position.total_fees_paid = opening_fee;
    
    // Asset amounts
    position.locked_amount = required_liquidity;
    position.collateral_amount = params.collateral_amount;
    
    // TP/SL
    position.take_profit_price = params.take_profit_price;
    position.stop_loss_price = params.stop_loss_price;
    
    // Limit order specific
    position.trigger_price = params.trigger_price;
    position.trigger_above_threshold = params.trigger_above_threshold;
    
    position.bump = ctx.bumps.position;
    
    // Update pool open interest
    if params.side == Side::Long {
        pool.long_open_interest_usd = math::checked_add(pool.long_open_interest_usd, params.size_usd as u128)?;
    } else {
        pool.short_open_interest_usd = math::checked_add(pool.short_open_interest_usd, params.size_usd as u128)?;
    }
    
    // Update user stats
    user.perp_position_count = user.perp_position_count.checked_add(1).unwrap_or(1);
    
    msg!("Successfully opened perpetual position");
    msg!("Position Type: {:?}", params.position_type);
    msg!("Side: {:?}", params.side);
    msg!("Size USD: {}", params.size_usd);
    msg!("Collateral USD: {}", collateral_usd);
    msg!("Leverage: {}x", leverage);
    
    Ok(())
}

fn calculate_liquidation_price(
    entry_price: u64,
    maintenance_margin_bps: u64,
    side: Side
) -> Result<u64> {
    let entry_price_f64 = math::checked_float_div(entry_price as f64, crate::math::PRICE_SCALE as f64)?;
    let margin_ratio = maintenance_margin_bps as f64 / 10_000.0;
    
    let liquidation_price_f64 = match side {
        Side::Long => {
            // Long liquidation: price falls by margin ratio
            math::checked_float_mul(entry_price_f64, 1.0 - margin_ratio)?
        },
        Side::Short => {
            // Short liquidation: price rises by margin ratio
            math::checked_float_mul(entry_price_f64, 1.0 + margin_ratio)?
        }
    };
    
    f64_to_scaled_price(liquidation_price_f64)
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
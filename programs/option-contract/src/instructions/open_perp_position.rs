use crate::{
    errors::OptionError,
    math,
    state::{Contract, Custody, OraclePrice, Pool, User, OptionDetail},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct OpenPerpPositionParams {
    pub collateral_amount: u64,  // Amount to use as collateral (in the collateral asset)
    pub position_size: u64,      // SOL amount to trade (leveraged)
    pub side: PerpSide,          // Long or Short
    pub max_slippage: u64,       // Max acceptable price slippage in basis points (100 = 1%)
    pub pool_name: String,       // Pool name (e.g., "SOL/USDC")
    pub pay_sol: bool,           // true = pay with SOL, false = pay with USDC
    pub pay_amount: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq)]
pub enum PerpSide {
    Long,  // Betting SOL price goes up
    Short, // Betting SOL price goes down
}

#[account]
pub struct PerpPosition {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub sol_custody: Pubkey,
    pub usdc_custody: Pubkey,
    
    // Position details
    pub side: PerpSide,
    pub collateral_amount: u64,    // Collateral amount in the collateral asset
    pub collateral_asset: Pubkey,  // Which asset is used as collateral (SOL or USDC custody)
    pub position_size: u64,        // SOL position size
    pub leverage: f64,             // Calculated leverage
    pub entry_price: f64,          // SOL price when opened
    pub liquidation_price: f64,    // Price at which position gets liquidated
    
    // Tracking
    pub open_time: i64,
    pub last_update_time: i64,
    pub unrealized_pnl: i64,       // Positive or negative P&L in USD
    
    // Risk management
    pub margin_ratio: f64,         // Current margin ratio
    pub is_liquidated: bool,
    
    pub bump: u8,
}

impl PerpPosition {
    pub const LEN: usize = 8 + 32 + 32 + 32 + 32 + 1 + 8 + 32 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 1 + 1 + 32; // Updated size
    
    pub const MAX_LEVERAGE: f64 = 100.0;
    pub const LIQUIDATION_THRESHOLD: f64 = 0.005; // 0.5% margin ratio triggers liquidation
    pub const MAINTENANCE_MARGIN: f64 = 0.10;    // 10% minimum margin ratio
    
    pub fn update_position(&mut self, current_price: f64, current_time: i64, collateral_price: f64) -> Result<()> {
        // Calculate unrealized P&L in USD
        // P&L = (price_diff / entry_price) * position_value * leverage
        let price_diff = match self.side {
            PerpSide::Long => current_price - self.entry_price,
            PerpSide::Short => self.entry_price - current_price,
        };
        
        let position_value_usd = self.position_size as f64 / 1_000_000_000.0;
        
        let pnl_ratio = math::checked_float_div(price_diff, self.entry_price)?;
        let unrealized_pnl_usd = math::checked_float_mul(pnl_ratio, position_value_usd)?;
        
        self.unrealized_pnl = (unrealized_pnl_usd * 1_000_000.0) as i64; // Store as micro-USD
        
        // Update margin ratio
        let collateral_decimals = if self.collateral_asset == self.sol_custody { 9 } else { 6 };
        let collateral_value_usd = math::checked_float_mul(
            self.collateral_amount as f64 / math::checked_powi(10.0, collateral_decimals)?,
            collateral_price
        )?;
        
        let current_equity = collateral_value_usd + unrealized_pnl_usd;
        self.margin_ratio = math::checked_float_div(current_equity, position_value_usd)?;
        
        self.last_update_time = current_time;
        
        Ok(())
    }
}

pub fn open_perp_position(
    ctx: Context<OpenPerpPosition>, 
    params: &OpenPerpPositionParams
) -> Result<()> {
    msg!("Opening SOL/USDC perpetual position");
    
    let owner = &ctx.accounts.owner;
    let contract = &ctx.accounts.contract;
    let pool = &ctx.accounts.pool;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    let position = &mut ctx.accounts.position;
    let user = &mut ctx.accounts.user;
    
    // Basic validation
    require!(params.collateral_amount > 0, OptionError::InvalidAmount);
    require!(params.position_size > 0, OptionError::InvalidAmount);
    require!(params.max_slippage <= 1000, OptionError::InvalidSlippage); // Max 10%
    require!(!params.pool_name.is_empty(), OptionError::InvalidPoolName);
    
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
    
    msg!("SOL Price: {}", sol_price.get_price());
    msg!("USDC Price: {}", usdc_price.get_price());
    
    let sol_price_value = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();
    
    // Determine collateral asset and custody based on position side and payment preference
    let (collateral_custody, collateral_token_account, collateral_decimals, collateral_price) = 
    match params.side {
        PerpSide::Long => {
            // Long with SOL collateral
            (sol_custody.key(), &ctx.accounts.sol_custody_token_account, sol_custody.decimals, sol_price_value)
        },
        PerpSide::Short => {
            // Short with USDC collateral (converted to SOL equivalent)
            (usdc_custody.key(), &ctx.accounts.usdc_custody_token_account, usdc_custody.decimals, usdc_price_value)
        },
    };

    
    msg!("Sol Custody : {}", sol_custody.key());
    msg!("USDC Custody : {}", usdc_custody.key());
    
    // Calculate position value and leverage
    let position_value_usd = params.position_size as f64 / math::checked_powi(10.0, sol_custody.decimals as i32)?;
    
    let collateral_value_usd = params.collateral_amount as f64 / math::checked_powi(10.0, collateral_decimals as i32)?;

    
    let (token_locked, token_owned) = (sol_custody.token_locked, sol_custody.token_owned);
    
    let borrow_rate = OptionDetail::get_sol_borrow_rate(token_locked, token_owned)?;
    let leverage = math::checked_float_div(
        position_value_usd * (1.0 - borrow_rate / 24.0 / 365.0), 
        collateral_value_usd
    )?;  
    msg!("Position Value: ${}", position_value_usd);
    msg!("Collateral Value: ${}", collateral_value_usd);
    msg!("Calculated Leverage: {}x", leverage);
    msg!("Pay Amount: {}x", params.pay_amount);
    
    // Validate leverage
    require!(
        leverage <= PerpPosition::MAX_LEVERAGE && leverage >= 1.0,
        OptionError::InvalidLeverage
    );
    
    // Check user has sufficient balance
    require_gte!(
        ctx.accounts.funding_account.amount,
        params.collateral_amount,
        OptionError::InsufficientBalance
    );
    
    // Calculate liquidation price
    let liquidation_price = calculate_liquidation_price(
        sol_price_value,
        leverage,
        params.side
    )?;
    
    msg!("Liquidation Price: ${}", liquidation_price);
    
    // Validate liquidation price makes sense
    match params.side {
        PerpSide::Long => {
            require!(
                liquidation_price < sol_price_value,
                OptionError::InvalidLiquidationPrice
            );
        },
        PerpSide::Short => {
            require!(
                liquidation_price > sol_price_value,
                OptionError::InvalidLiquidationPrice
            );
        }
    }
    
    // Check if we have enough liquidity in the pool
    match params.side {
        PerpSide::Long => {
            // For long positions, we need SOL liquidity to back the position
            require_gte!(
                sol_custody.token_owned,
                params.position_size,
                OptionError::InsufficientPoolLiquidity
            );
        },
        PerpSide::Short => {
            // For short positions, we need USDC liquidity
            let required_usdc = math::checked_as_u64(
                position_value_usd * math::checked_powi(10.0, usdc_custody.decimals as i32)?
            )?;
            require_gte!(
                usdc_custody.token_owned,
                required_usdc,
                OptionError::InsufficientPoolLiquidity
            );
        }
    }
    
    // Transfer collateral from user to pool
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            SplTransfer {
                from: ctx.accounts.funding_account.to_account_info(),
                to: collateral_token_account.to_account_info(),
                authority: owner.to_account_info(),
            },
        ),
        params.pay_amount,
    )?;
    
    // Lock the corresponding assets in custody and update stats
    match params.side {
        PerpSide::Long => {
            // Lock SOL tokens for long position
            sol_custody.token_locked = math::checked_add(
                sol_custody.token_locked, 
                params.position_size
            )?;
        },
        PerpSide::Short => {
            // Lock USDC equivalent for short position  
            let usdc_to_lock = math::checked_as_u64(
                position_value_usd * math::checked_powi(10.0, usdc_custody.decimals as i32)?
            )?;
            usdc_custody.token_locked = math::checked_add(
                usdc_custody.token_locked,
                usdc_to_lock
            )?;
        }
    }
    
    // Update custody stats for the collateral asset
    if params.side == PerpSide::Long {
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
    
    // Initialize position
    position.owner = owner.key();
    position.pool = pool.key();
    position.sol_custody = sol_custody.key();
    position.usdc_custody = usdc_custody.key();
    position.side = params.side;
    position.collateral_amount = params.collateral_amount;
    position.collateral_asset = collateral_custody;
    position.position_size = params.position_size;
    position.leverage = leverage;
    position.entry_price = sol_price_value;
    position.liquidation_price = liquidation_price;
    position.open_time = current_time;
    position.last_update_time = current_time;
    position.unrealized_pnl = 0;
    position.margin_ratio = 1.0 / leverage; // Initial margin ratio
    position.is_liquidated = false;
    position.bump = ctx.bumps.position;
    
    // Update user stats
    user.perp_position_count = user.perp_position_count.checked_add(1).unwrap_or(1);
    
    msg!("Successfully opened perpetual position");
    msg!("Entry Price: ${}", position.entry_price);
    msg!("Liquidation Price: ${}", position.liquidation_price);
    msg!("Leverage: {}x", position.leverage);
    msg!("Collateral Asset: {}", if params.pay_sol { "SOL" } else { "USDC" });
    
    Ok(())
}

// Helper function to calculate liquidation price
fn calculate_liquidation_price(
    entry_price: f64,
    leverage: f64,
    side: PerpSide
) -> Result<f64> {
    let liquidation_threshold = PerpPosition::LIQUIDATION_THRESHOLD; // 0.05%
    
    // Calculate price movement to liquidation
    let price_movement_ratio = math::checked_float_sub(1.0 / leverage, liquidation_threshold)?;
    
    match side {
        PerpSide::Long => {
            // Long liquidation: price falls
            math::checked_float_mul(
                entry_price,
                math::checked_float_sub(1.0, price_movement_ratio)?
            )
        },
        PerpSide::Short => {
            // Short liquidation: price rises  
            math::checked_float_mul(
                entry_price,
                math::checked_float_add(1.0, price_movement_ratio)?
            )
        }
    }
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
    pub funding_account: Box<Account<'info, TokenAccount>>, // User's payment account (SOL or USDC)

    /// CHECK: empty PDA, authority for token accounts
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
        seeds = [b"user_v2", owner.key().as_ref()],
        bump,
    )]
    pub user: Box<Account<'info, User>>,

    #[account(
        init,
        payer = owner,
        space = PerpPosition::LEN,
        seeds = [
            b"perp_position",
            owner.key().as_ref(),
            (user.perp_position_count + 1).to_le_bytes().as_ref(),
            pool.key().as_ref()
        ],
        bump
    )]
    pub position: Box<Account<'info, PerpPosition>>,

    // SOL custody (for position backing)
    #[account(
        mut,
        seeds = [b"custody", pool.key().as_ref(), sol_mint.key().as_ref()],
        bump = sol_custody.bump
    )]
    pub sol_custody: Box<Account<'info, Custody>>,

    // USDC custody (for collateral)
    #[account(
        mut,
        seeds = [b"custody", pool.key().as_ref(), usdc_mint.key().as_ref()],
        bump = usdc_custody.bump
    )]
    pub usdc_custody: Box<Account<'info, Custody>>,

    // SOL token account (pool's SOL vault)
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

    // USDC token account (pool's USDC vault)
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

    /// CHECK: SOL oracle account
    #[account(
        constraint = sol_oracle_account.key() == sol_custody.oracle
    )]
    pub sol_oracle_account: AccountInfo<'info>,

    /// CHECK: USDC oracle account  
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
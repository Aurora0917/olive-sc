use crate::{
    errors::{PerpetualError, TradingError},
    events::PositionSizeUpdated,
    math::{self, f64_to_scaled_price},
    utils::risk_management::*,
    state::{Contract, Custody, OraclePrice, Pool, Position, Side, OrderType},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UpdatePositionSizeParams {
    pub position_index: u64,
    pub pool_name: String,
    pub is_increase: bool,              // true = increase size, false = decrease size
    pub size_delta_usd: u64,            // Amount to increase/decrease in USD (6 decimals)
    pub collateral_delta: u64,          // Additional collateral if increasing (in tokens)
    pub pay_sol: bool,                  // For increases: true = pay with SOL, false = pay with USDC
    pub receive_sol: bool,              // For decreases: true = receive SOL, false = receive USDC
}

pub fn update_position_size(
    ctx: Context<UpdatePositionSize>,
    params: &UpdatePositionSizeParams
) -> Result<()> {
    msg!("Updating position size");
    
    let owner = &ctx.accounts.owner;
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    let position = &mut ctx.accounts.position;
    
    // Validation
    require_keys_eq!(position.owner, owner.key(), TradingError::Unauthorized);
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require!(position.order_type == OrderType::Market, PerpetualError::InvalidOrderType);
    require!(params.size_delta_usd > 0, TradingError::InvalidAmount);
    
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
    let current_price_scaled = f64_to_scaled_price(sol_price_value)?;
    
    msg!("SOL Price: {}", sol_price_value);
    msg!("USDC Price: {}", usdc_price_value);
    msg!("{} position size by {} USD", if params.is_increase { "Increasing" } else { "Decreasing" }, params.size_delta_usd);
    
    // Store previous values for event
    let previous_size_usd = position.size_usd;
    let previous_collateral_usd = position.collateral_usd;
    let previous_locked_amount = position.locked_amount;
    
    if params.is_increase {
        // Increase position size
        require!(params.collateral_delta > 0, TradingError::InvalidAmount);
        
        // Check user has sufficient balance
        require_gte!(
            ctx.accounts.funding_account.amount,
            params.collateral_delta,
            TradingError::InsufficientBalance
        );
        
        // Determine collateral asset and calculate USD value
        let (collateral_custody, collateral_decimals, collateral_price) = 
            if params.pay_sol {
                (sol_custody.key(), sol_custody.decimals, sol_price_value)
            } else {
                (usdc_custody.key(), usdc_custody.decimals, usdc_price_value)
            };
        
        // Validate collateral asset matches position
        require_keys_eq!(position.collateral_custody, collateral_custody, PerpetualError::InvalidCollateralAsset);
        
        // Calculate collateral USD value
        let collateral_usd_delta = math::checked_as_u64(math::checked_float_mul(
            params.collateral_delta as f64 / math::checked_powi(10.0, collateral_decimals as i32)?,
            collateral_price
        )? * 1_000_000.0)?;
        
        // Calculate new position values
        let new_size_usd = math::checked_add(position.size_usd, params.size_delta_usd)?;
        let new_collateral_usd = math::checked_add(position.collateral_usd, collateral_usd_delta)?;
        
        // Calculate required liquidity for the size delta
        let required_liquidity_delta = if position.side == Side::Long {
            let usd_amount = params.size_delta_usd as f64 / 1_000_000.0;
            let sol_tokens_needed = usd_amount / sol_price_value;
            math::checked_as_u64(sol_tokens_needed * math::checked_powi(10.0, sol_custody.decimals as i32)?)?
        } else {
            let usd_amount = params.size_delta_usd as f64 / 1_000_000.0;
            let usdc_tokens_needed = usd_amount / usdc_price_value;
            math::checked_as_u64(usdc_tokens_needed * math::checked_powi(10.0, usdc_custody.decimals as i32)?)?
        };
        
        // Check pool liquidity
        if position.side == Side::Long {
            require_gte!(
                sol_custody.token_owned.saturating_sub(sol_custody.token_locked),
                required_liquidity_delta,
                TradingError::InsufficientPoolLiquidity
            );
        } else {
            require_gte!(
                usdc_custody.token_owned.saturating_sub(usdc_custody.token_locked),
                required_liquidity_delta,
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
            params.collateral_delta,
        )?;
        
        // Update custody stats
        if position.side == Side::Long {
            sol_custody.token_locked = math::checked_add(
                sol_custody.token_locked,
                required_liquidity_delta
            )?;
        } else {
            usdc_custody.token_locked = math::checked_add(
                usdc_custody.token_locked,
                required_liquidity_delta
            )?;
        }
        
        if params.pay_sol {
            sol_custody.token_owned = math::checked_add(
                sol_custody.token_owned,
                params.collateral_delta
            )?;
        } else {
            usdc_custody.token_owned = math::checked_add(
                usdc_custody.token_owned,
                params.collateral_delta
            )?;
        }
        
        // Update position
        position.size_usd = new_size_usd;
        position.collateral_usd = new_collateral_usd;
        position.collateral_amount = math::checked_add(
            position.collateral_amount,
            params.collateral_delta
        )?;
        position.locked_amount = math::checked_add(
            position.locked_amount,
            required_liquidity_delta
        )?;
        
        // Update pool open interest
        if position.side == Side::Long {
            pool.long_open_interest_usd = math::checked_add(
                pool.long_open_interest_usd, 
                params.size_delta_usd as u128
            )?;
        } else {
            pool.short_open_interest_usd = math::checked_add(
                pool.short_open_interest_usd, 
                params.size_delta_usd as u128
            )?;
        }
        
    } else {
        // Decrease position size
        require!(params.size_delta_usd < position.size_usd, TradingError::InvalidAmount);
        
        // Calculate proportional collateral reduction
        let size_reduction_ratio = math::checked_div(params.size_delta_usd as u128, position.size_usd as u128)?;
        let collateral_usd_to_return = math::checked_as_u64(math::checked_mul(
            position.collateral_usd as u128,
            size_reduction_ratio
        )?)?;
        let collateral_amount_to_return = math::checked_as_u64(math::checked_mul(
            position.collateral_amount as u128,
            size_reduction_ratio
        )?)?;
        let locked_amount_to_release = math::checked_as_u64(math::checked_mul(
            position.locked_amount as u128,
            size_reduction_ratio
        )?)?;
        
        // Calculate new position values
        let new_size_usd = math::checked_sub(position.size_usd, params.size_delta_usd)?;
        let new_collateral_usd = math::checked_sub(position.collateral_usd, collateral_usd_to_return)?;
        
        // Ensure minimum position size
        require!(new_size_usd >= 1_000_000, TradingError::PositionTooSmall); // Min $1
        
        // Calculate PnL for the portion being closed
        let pnl = position.calculate_pnl(current_price_scaled)?;
        let proportional_pnl = math::checked_as_i64(math::checked_mul(
            pnl as i128,
            size_reduction_ratio as i128
        )? / 10_000)?; // Divide by 10000 to account for basis points
        
        // Calculate settlement amount
        let settlement_usd = if proportional_pnl >= 0 {
            math::checked_add(collateral_usd_to_return, proportional_pnl as u64)?
        } else {
            let loss = (-proportional_pnl) as u64;
            collateral_usd_to_return.saturating_sub(loss)
        };
        
        // Calculate withdrawal tokens
        let (withdrawal_tokens, withdrawal_decimals) = if params.receive_sol {
            let amount = settlement_usd as f64 / sol_price_value;
            (amount, sol_custody.decimals)
        } else {
            let amount = settlement_usd as f64 / usdc_price_value;
            (amount, usdc_custody.decimals)
        };
        
        let withdrawal_token_amount = math::checked_as_u64(
            withdrawal_tokens * math::checked_powi(10.0, withdrawal_decimals as i32)?
        )?;
        
        // Transfer settlement to user
        if withdrawal_token_amount > 0 {
            ctx.accounts.contract.transfer_tokens(
                if params.receive_sol {
                    ctx.accounts.sol_custody_token_account.to_account_info()
                } else {
                    ctx.accounts.usdc_custody_token_account.to_account_info()
                },
                ctx.accounts.receiving_account.to_account_info(),
                ctx.accounts.transfer_authority.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
                withdrawal_token_amount,
            )?;
        }
        
        // Update custody stats
        if position.side == Side::Long {
            sol_custody.token_locked = math::checked_sub(
                sol_custody.token_locked,
                locked_amount_to_release
            )?;
        } else {
            usdc_custody.token_locked = math::checked_sub(
                usdc_custody.token_locked,
                locked_amount_to_release
            )?;
        }
        
        if position.collateral_custody == sol_custody.key() {
            sol_custody.token_owned = math::checked_sub(
                sol_custody.token_owned,
                collateral_amount_to_return
            )?;
        } else {
            usdc_custody.token_owned = math::checked_sub(
                usdc_custody.token_owned,
                collateral_amount_to_return
            )?;
        }
        
        // Update position
        position.size_usd = new_size_usd;
        position.collateral_usd = new_collateral_usd;
        position.collateral_amount = math::checked_sub(
            position.collateral_amount,
            collateral_amount_to_return
        )?;
        position.locked_amount = math::checked_sub(
            position.locked_amount,
            locked_amount_to_release
        )?;
        
        // Update pool open interest
        if position.side == Side::Long {
            pool.long_open_interest_usd = math::checked_sub(
                pool.long_open_interest_usd, 
                params.size_delta_usd as u128
            )?;
        } else {
            pool.short_open_interest_usd = math::checked_sub(
                pool.short_open_interest_usd, 
                params.size_delta_usd as u128
            )?;
        }
    }
    
    let new_leverage = math::checked_float_div(position.size_usd as f64, position.collateral_usd as f64)?;
    
    // Recalculate liquidation price
    let new_liquidation_price = calculate_liquidation_price(
        position.price,
        new_leverage,
        position.side
    )?;
    
    // Update accrued borrow fees before modifying position
    pool.update_position_borrow_fees(position, current_time, sol_custody, usdc_custody)?;
    
    position.liquidation_price = new_liquidation_price;
    position.update_time = current_time;
    
    msg!("Position size updated successfully");
    msg!("New size USD: {}", position.size_usd);
    msg!("New collateral USD: {}", position.collateral_usd);
    msg!("New leverage: {}x", new_leverage);
    msg!("New liquidation price: {}", position.liquidation_price);
    msg!("New locked amount: {}", position.locked_amount);
    
    emit!(PositionSizeUpdated {
        owner: owner.key(),
        position_index: params.position_index,
        pool: pool.key(),
        custody: position.custody,
        collateral_custody: position.collateral_custody,
        order_type: position.order_type as u8,
        side: position.side as u8,
        is_increase: params.is_increase,
        size_delta_usd: params.size_delta_usd,
        collateral_delta: if params.is_increase { params.collateral_delta } else { 0 },
        previous_size_usd,
        new_size_usd: position.size_usd,
        previous_collateral_usd,
        new_collateral_usd: position.collateral_usd,
        new_leverage,
        new_liquidation_price: position.liquidation_price,
        locked_amount_delta: if params.is_increase { 
            position.locked_amount - previous_locked_amount 
        } else { 
            previous_locked_amount - position.locked_amount 
        },
        new_locked_amount: position.locked_amount,
        update_time: current_time,
    });
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: UpdatePositionSizeParams)]
pub struct UpdatePositionSize<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = owner
    )]
    pub funding_account: Box<Account<'info, TokenAccount>>,
    
    #[account(
        mut,
        has_one = owner
    )]
    pub receiving_account: Box<Account<'info, TokenAccount>>,

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
        mut,
        seeds = [
            b"position",
            owner.key().as_ref(),
            params.position_index.to_le_bytes().as_ref(),
            pool.key().as_ref()
        ],
        bump = position.bump
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
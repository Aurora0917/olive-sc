use crate::{
    errors::{PerpetualError, TradingError},
    events::CollateralAdded,
    math::{self},
    utils::risk_management::*,
    state::{Contract, Custody, OraclePrice, Pool, Position, OrderType},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer as SplTransfer};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AddCollateralParams {
    pub position_index: u64,
    pub pool_name: String,
    pub collateral_amount: u64,  // Amount of collateral to add
    pub pay_sol: bool,           // true = add SOL, false = add USDC
}

pub fn add_collateral(
    ctx: Context<AddCollateral>,
    params: &AddCollateralParams
) -> Result<()> {
    msg!("Adding collateral to perpetual position");
    
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let position = &mut ctx.accounts.position;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    
    // Validation
    require_keys_eq!(position.owner, ctx.accounts.owner.key(), TradingError::Unauthorized);
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require!(position.order_type == OrderType::Market, PerpetualError::InvalidOrderType);
    require!(params.collateral_amount > 0, TradingError::InvalidAmount);
    
    // Check user has sufficient balance
    require_gte!(
        ctx.accounts.funding_account.amount,
        params.collateral_amount,
        TradingError::InsufficientBalance
    );
    
    // Get current prices
    let current_time = contract.get_time()?;
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let sol_price_value = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();
    
    msg!("SOL Price: {}", sol_price_value);
    msg!("USDC Price: {}", usdc_price_value);
    msg!("Adding {} tokens as collateral", params.collateral_amount);
    
    // Determine collateral asset and calculate USD value
    let (collateral_decimals, collateral_price) = 
        if params.pay_sol {
            (sol_custody.decimals, sol_price_value)
        } else {
            (usdc_custody.decimals, usdc_price_value)
        };
    
    // Calculate USD value of added collateral (scaled to 6 decimals)
    let collateral_usd_to_add = math::checked_as_u64(math::checked_float_mul(
        params.collateral_amount as f64 / math::checked_powi(10.0, collateral_decimals as i32)?,
        collateral_price
    )? * 1_000_000.0)?;
    
    msg!("Collateral USD to add: {}", collateral_usd_to_add);
    msg!("Current collateral USD: {}", position.collateral_usd);
    
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
                authority: ctx.accounts.owner.to_account_info(),
            },
        ),
        params.collateral_amount,
    )?;
    
    // Update custody stats based on what asset was actually added
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
    
    
    // Update position collateral
    position.collateral_usd = math::checked_add(
        position.collateral_usd,
        collateral_usd_to_add
    )?;
    
    // Convert and add to collateral_amount based on position's collateral custody
    if position.collateral_custody == sol_custody.key() {
        // Position stores collateral in SOL
        let sol_amount_to_add = if params.pay_sol {
            // Adding SOL to SOL position - direct add
            params.collateral_amount
        } else {
            // Adding USDC to SOL position - convert USDC to SOL
            let usd_actual = collateral_usd_to_add as f64 / 1_000_000.0;
            let sol_value = usd_actual / sol_price_value;
            math::checked_as_u64(sol_value * math::checked_powi(10.0, sol_custody.decimals as i32)?)?
        };
        position.collateral_amount = math::checked_add(
            position.collateral_amount,
            sol_amount_to_add
        )?;
    } else {
        // Position stores collateral in USDC
        let usdc_amount_to_add = if params.pay_sol {
            // Adding SOL to USDC position - convert SOL to USDC
            let usd_actual = collateral_usd_to_add as f64 / 1_000_000.0;
            let usdc_value = usd_actual / usdc_price_value;
            math::checked_as_u64(usdc_value * math::checked_powi(10.0, usdc_custody.decimals as i32)?)?
        } else {
            // Adding USDC to USDC position - direct add
            params.collateral_amount
        };
        position.collateral_amount = math::checked_add(
            position.collateral_amount,
            usdc_amount_to_add
        )?;
    }
    
    // Recalculate borrow size (position size - collateral)
    position.borrow_size_usd = position.size_usd.saturating_sub(position.collateral_usd);
    
    // Recalculate margin requirements based on new collateral
    let new_leverage = std::cmp::max(math::checked_div(position.size_usd, position.collateral_usd)?, 1);
    
    // Recalculate liquidation price with new margin
    let new_liquidation_price = calculate_liquidation_price(
        position.price,
        new_leverage,
        position.side
    )?;
    
    position.liquidation_price = new_liquidation_price;
    position.update_time = current_time;
    
    msg!("Successfully added collateral");
    msg!("New collateral amount: {}", position.collateral_amount);
    msg!("New collateral USD: {}", position.collateral_usd);
    msg!("New leverage: {}x", new_leverage);
    msg!("New liquidation price: {}", position.liquidation_price);
    msg!("New borrow size USD: {}", position.borrow_size_usd);
    
    emit!(CollateralAdded {
        pub_key: position.key(),
        owner: ctx.accounts.owner.key(),
        position_index: params.position_index,
        pool: pool.key(),
        custody: position.custody,
        collateral_custody: position.collateral_custody,
        order_type: position.order_type as u8,
        side: position.side as u8,
        collateral_amount_added: params.collateral_amount,
        collateral_usd_added: collateral_usd_to_add,
        new_collateral_amount: position.collateral_amount,
        new_collateral_usd: position.collateral_usd,
        new_leverage,
        new_liquidation_price: position.liquidation_price,
        new_borrow_size_usd: position.borrow_size_usd,
        update_time: current_time,
    });
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: AddCollateralParams)]
pub struct AddCollateral<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = owner
    )]
    pub funding_account: Box<Account<'info, TokenAccount>>,

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
}
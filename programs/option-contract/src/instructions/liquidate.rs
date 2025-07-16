use crate::{
    errors::PerpetualError,
    math::{self, f64_to_scaled_price},
    state::{Contract, Custody, OraclePrice, Pool, Position, Side, PositionType},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct LiquidateParams {
    pub position_index: u64,
    pub pool_name: String,
    pub liquidator_reward_account: Pubkey, // Account to receive liquidator reward
}

pub fn liquidate(
    ctx: Context<Liquidate>,
    _params: &LiquidateParams
) -> Result<()> {
    msg!("Liquidating perpetual position");
    
    let contract = &ctx.accounts.contract;
    let pool = &mut ctx.accounts.pool;
    let position = &mut ctx.accounts.position;
    let sol_custody = &mut ctx.accounts.sol_custody;
    let usdc_custody = &mut ctx.accounts.usdc_custody;
    
    // Validation
    require!(!position.is_liquidated, PerpetualError::PositionLiquidated);
    require!(position.position_type == PositionType::Market, PerpetualError::InvalidPositionType);
    
    // Get current prices from oracles
    let current_time = contract.get_time()?;
    let sol_price = OraclePrice::new_from_oracle(&ctx.accounts.sol_oracle_account, current_time, false)?;
    let usdc_price = OraclePrice::new_from_oracle(&ctx.accounts.usdc_oracle_account, current_time, false)?;
    
    let current_sol_price = sol_price.get_price();
    let usdc_price_value = usdc_price.get_price();
    let current_price_scaled = f64_to_scaled_price(current_sol_price)?;
    
    msg!("SOL Price: {}", current_sol_price);
    msg!("USDC Price: {}", usdc_price_value);
    msg!("Liquidating position owned by: {}", position.owner);
    msg!("Position entry price: {}", position.price);
    msg!("Position liquidation price: {}", position.liquidation_price);
    msg!("Position side: {:?}", position.side);
    
    // Check if position can be liquidated by price
    let price_liquidatable = position.is_liquidatable(current_price_scaled);
    
    // Check if position can be liquidated by margin ratio
    let margin_liquidatable = position.is_liquidatable_by_margin(current_price_scaled)?;
    
    // Must be liquidatable by either price or margin
    require!(
        price_liquidatable || margin_liquidatable,
        PerpetualError::PositionNotLiquidatable
    );
    
    msg!("Position is eligible for liquidation");
    msg!("Price liquidatable: {}", price_liquidatable);
    msg!("Margin liquidatable: {}", margin_liquidatable);
    
    // Calculate P&L
    let pnl = position.calculate_pnl(current_price_scaled)?;
    
    // Calculate funding and interest payments
    let funding_payment = pool.get_funding_payment(
        position.side == Side::Long,
        position.size_usd as u128,
        position.cumulative_funding_snapshot.try_into().unwrap()
    )?;
    let interest_payment = pool.get_interest_payment(
        position.borrow_size_usd as u128,
        position.cumulative_interest_snapshot
    )?;
    
    // Calculate liquidator reward (0.5% of position size)
    let liquidator_reward_usd = math::checked_div(position.size_usd, 200)?; // 0.5%
    
    // Calculate net settlement after all deductions
    let mut net_settlement = position.collateral_usd as i64 + pnl - funding_payment as i64 - interest_payment as i64 - liquidator_reward_usd as i64;
    
    // Ensure settlement is not negative
    if net_settlement < 0 {
        net_settlement = 0;
    }
    
    let settlement_usd = net_settlement as u64;
    
    msg!("P&L: {}", pnl);
    msg!("Funding payment: {}", funding_payment);
    msg!("Interest payment: {}", interest_payment);
    msg!("Liquidator reward USD: {}", liquidator_reward_usd);
    msg!("Net settlement USD: {}", settlement_usd);
    
    // Calculate settlement amounts in tokens
    let (collateral_price, collateral_decimals) = if position.collateral_custody == sol_custody.key() {
        (current_sol_price, sol_custody.decimals)
    } else {
        (usdc_price_value, usdc_custody.decimals)
    };
    
    // Settlement to position owner
    let settlement_tokens = if settlement_usd > 0 {
        let amount = math::checked_as_u64(settlement_usd as f64 / collateral_price)?;
        math::checked_as_u64(amount as f64 * math::checked_powi(10.0, collateral_decimals as i32)?)?
    } else {
        0
    };
    
    // Liquidator reward tokens
    let liquidator_reward_tokens = if liquidator_reward_usd > 0 {
        let amount = math::checked_as_u64(liquidator_reward_usd as f64 / collateral_price)?;
        math::checked_as_u64(amount as f64 * math::checked_powi(10.0, collateral_decimals as i32)?)?
    } else {
        0
    };
    
    // Transfer settlement to position owner if any
    if settlement_tokens > 0 {
        ctx.accounts.contract.transfer_tokens(
            if position.collateral_custody == sol_custody.key() {
                ctx.accounts.sol_custody_token_account.to_account_info()
            } else {
                ctx.accounts.usdc_custody_token_account.to_account_info()
            },
            ctx.accounts.owner_settlement_account.to_account_info(),
            ctx.accounts.transfer_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            settlement_tokens,
        )?;
    }
    
    // Transfer liquidator reward
    if liquidator_reward_tokens > 0 {
        ctx.accounts.contract.transfer_tokens(
            if position.collateral_custody == sol_custody.key() {
                ctx.accounts.sol_custody_token_account.to_account_info()
            } else {
                ctx.accounts.usdc_custody_token_account.to_account_info()
            },
            ctx.accounts.liquidator_reward_account.to_account_info(),
            ctx.accounts.transfer_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            liquidator_reward_tokens,
        )?;
    }
    
    // Update custody stats - release locked tokens
    if position.side == Side::Long {
        sol_custody.token_locked = math::checked_sub(
            sol_custody.token_locked,
            position.locked_amount
        )?;
    } else {
        usdc_custody.token_locked = math::checked_sub(
            usdc_custody.token_locked,
            position.locked_amount
        )?;
    }
    
    // Update custody ownership - remove collateral
    if position.collateral_custody == sol_custody.key() {
        sol_custody.token_owned = math::checked_sub(
            sol_custody.token_owned,
            position.collateral_amount
        )?;
    } else {
        usdc_custody.token_owned = math::checked_sub(
            usdc_custody.token_owned,
            position.collateral_amount
        )?;
    }
    
    // Mark position as liquidated
    position.is_liquidated = true;
    position.size_usd = 0;
    position.collateral_amount = 0;
    position.collateral_usd = 0;
    position.locked_amount = 0;
    position.update_time = current_time;
    
    // Update fee tracking
    let liquidation_fee = math::checked_div(position.size_usd, 100)?; // 1% liquidation fee
    position.total_fees_paid = math::checked_add(position.total_fees_paid, liquidation_fee)?;
    position.total_fees_paid = math::checked_add(position.total_fees_paid, interest_payment.try_into().unwrap())?;
    position.total_fees_paid = math::checked_add(position.total_fees_paid, liquidator_reward_usd)?;
    
    msg!("Successfully liquidated position");
    msg!("Settlement to owner: {} tokens", settlement_tokens);
    msg!("Liquidator reward: {} tokens", liquidator_reward_tokens);
    msg!("Total fees paid: {}", position.total_fees_paid);
    
    Ok(())
}

#[derive(Accounts)]
#[instruction(params: LiquidateParams)]
pub struct Liquidate<'info> {
    #[account(mut)]
    pub liquidator: Signer<'info>,

    /// CHECK: Position owner for settlement
    #[account(mut)]
    pub owner_settlement_account: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub liquidator_reward_account: Box<Account<'info, TokenAccount>>,

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
        mut,
        seeds = [
            b"position",
            position.owner.as_ref(),
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
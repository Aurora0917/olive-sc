use crate::{
    errors::OptionError,
    math::{self, scaled_price_to_f64},
    state::{Contract, Custody, OptionDetail, OraclePrice, Pool, User},
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CloseOptionParams {
    pub option_index: u64,
    pub pool_name: String,
    pub close_quantity: u64,  // Number of option contracts to close
}

pub fn close_option(ctx: Context<CloseOption>, params: &CloseOptionParams) -> Result<()> {
    let token_program = &ctx.accounts.token_program;
    let option_detail = &mut ctx.accounts.option_detail;
    let closed_option_detail = &mut ctx.accounts.closed_option_detail;
    let contract = &ctx.accounts.contract;
    let user = &ctx.accounts.user;
    let pool = &ctx.accounts.pool;
    let custody = &ctx.accounts.custody;
    let transfer_authority = &ctx.accounts.transfer_authority;

    let locked_custody = &mut ctx.accounts.locked_custody;
    let pay_custody = &mut ctx.accounts.pay_custody;
    let locked_custody_token_account = &ctx.accounts.locked_custody_token_account;
    let funding_account = &ctx.accounts.funding_account;
    let _pay_custody_oracle_account = &ctx.accounts.pay_custody_oracle_account;
    let custody_oracle_account = &ctx.accounts.custody_oracle_account;
    let locked_oracle = &ctx.accounts.locked_oracle;

    require_keys_eq!(pay_custody.key(), option_detail.premium_asset);
    require_keys_eq!(locked_custody.key(), option_detail.locked_asset);
    require_gte!(user.option_index, params.option_index);
    
    // Validate close quantity
    require_gt!(params.close_quantity, 0, OptionError::InvalidQuantityError);
    require_gte!(option_detail.quantity, params.close_quantity, OptionError::InsufficientQuantityError);

    // Only if option is valid and not exercised
    if option_detail.valid {
        // Get current time and check that option has not expired
        let current_time: i64 = contract.get_time()?;
        if current_time >= option_detail.expired_date {
            return Err(OptionError::InvalidTimeError.into());
        }

        // Calculate proportional amounts for partial close
        let unlock_amount = math::checked_div(
            math::checked_mul(option_detail.amount, params.close_quantity)?,
            option_detail.quantity
        )?;

        // Validate locked custody has enough tokens
        require_gte!(
            locked_custody.token_locked,
            unlock_amount,
            OptionError::InvalidLockedBalanceError
        );

        // Time decay logic for Black-Scholes
        let remaining_seconds = option_detail.expired_date.saturating_sub(current_time);
        let remaining_days = remaining_seconds as f64 / 86400.0;
        let remaining_years = remaining_days / 365.0;

        // Oracle price of underlying asset (SOL)
        let underlying_price = OraclePrice::new_from_oracle(
            custody_oracle_account,
            current_time,
            false,
        )?.get_price();
        
        // Get utilization data for the option's underlying asset
        let (token_locked, token_owned) = (locked_custody.token_locked, locked_custody.token_owned);
        
        // Calculate Premium using enhanced Black-Scholes with dynamic borrow rate
        let bs_price_per_contract = OptionDetail::black_scholes_with_borrow_rate(
            underlying_price,
            scaled_price_to_f64(option_detail.strike_price)?,
            remaining_years,
            option_detail.option_type == 0, // call/put logic
            token_locked,  // Current utilization of underlying asset
            token_owned,   // Total supply of underlying asset
            option_detail.option_type == 0, // Asset type for rate calculation
        )?;

        // Calculate proportional premium for close quantity
        let bs_price_partial = bs_price_per_contract * params.close_quantity as f64;

        // Get locked token oracle price for USD to locked token conversion
        let locked_token_price = OraclePrice::new_from_oracle(
            locked_oracle,
            current_time,
            false,
        )?.get_price();

        // Convert USD option value to locked token amount using float math (like original code)
        let token_decimals = locked_custody.decimals;
        let refund_amount_raw = math::checked_as_u64(
            math::checked_float_div(bs_price_partial, locked_token_price)?
                * math::checked_powi(10.0, token_decimals as i32)?
        )?;

        // Debug logging to see actual values
        msg!("Black-Scholes per contract price: {}", bs_price_per_contract);
        msg!("Quantity partial price: {}", params.close_quantity);
        msg!("Quantity full price: {}", option_detail.quantity);
        msg!("Black-Scholes partial price: {}", bs_price_partial);
        msg!("Locked token price: {}", locked_token_price);
        msg!("Token decimals: {}", token_decimals);
        msg!("Refund amount raw: {}", refund_amount_raw);
        msg!("Strike price: {}", option_detail.strike_price);
        msg!("Current underlying price: {}", underlying_price);
        msg!("Option type (0=call, 1=put): {}", option_detail.option_type);
        msg!("Remaining years: {}", remaining_years);
        msg!("Original locked amount: {}", option_detail.amount);

        // Set minimum refund to prevent 0 amounts (at least 1 unit of the token)
        let min_refund = 1u64;
        let refund_amount_raw = if refund_amount_raw == 0 && bs_price_partial > 0.0 {
            min_refund
        } else {
            refund_amount_raw
        };

        require_gt!(refund_amount_raw, 0, OptionError::InvalidPayAmountError);

        // Apply 10% platform fee (90% refund)
        let refund_amount = math::checked_div(math::checked_mul(refund_amount_raw, 9)?, 10)?;

        // Check locked custody has enough balance for refund
        require_gte!(
            math::checked_sub(locked_custody.token_owned, locked_custody.token_locked)?,
            refund_amount,
            OptionError::InvalidPoolBalanceError
        );

        // Update locked custody balances
        locked_custody.token_owned = math::checked_sub(locked_custody.token_owned, refund_amount)?;
        locked_custody.token_locked = math::checked_sub(locked_custody.token_locked, unlock_amount)?;

        // Transfer refund to user (from locked asset pool)
        contract.transfer_tokens(
            locked_custody_token_account.to_account_info(),
            funding_account.to_account_info(),
            transfer_authority.to_account_info(),
            token_program.to_account_info(),
            refund_amount,
        )?;

        if option_detail.quantity == params.close_quantity {
            option_detail.valid = false;
            option_detail.bought_back = current_time as u64;
        } else {

            // Handle closed position tracking
            if closed_option_detail.quantity > 0 {
                
                msg!("Second Partial {}", closed_option_detail.quantity);
                // Accumulate to existing closed position
                closed_option_detail.quantity = math::checked_add(
                    closed_option_detail.quantity, 
                    params.close_quantity
                )?;
                closed_option_detail.amount = math::checked_add(
                    closed_option_detail.amount, 
                    unlock_amount
                )?;
                closed_option_detail.bought_back = current_time as u64; // Update to latest close time
            } else {
                
                msg!("Create Partial {}", params.close_quantity);
                // Initialize new closed position (first partial close) - following open_option.rs pattern
                closed_option_detail.valid = false; // Mark as closed position
                closed_option_detail.quantity = params.close_quantity;
                closed_option_detail.amount = unlock_amount;
                closed_option_detail.owner = option_detail.owner;
                closed_option_detail.index = option_detail.index;
                closed_option_detail.period = option_detail.period;
                closed_option_detail.expired_date = option_detail.expired_date;
                closed_option_detail.purchase_date = option_detail.purchase_date;
                closed_option_detail.option_type = option_detail.option_type;
                closed_option_detail.strike_price = option_detail.strike_price;
                closed_option_detail.premium_asset = option_detail.premium_asset;
                closed_option_detail.locked_asset = option_detail.locked_asset;
                closed_option_detail.pool = pool.key();
                closed_option_detail.custody = custody.key();
                closed_option_detail.premium = math::checked_div(
                    math::checked_mul(option_detail.premium, params.close_quantity)?,
                    option_detail.quantity
                )?; // Proportional premium for closed quantity
                closed_option_detail.bought_back = current_time as u64;
            }
            
            // Update original position (reduce by closed amount)
            option_detail.quantity = math::checked_sub(option_detail.quantity, params.close_quantity)?;
            option_detail.amount = math::checked_sub(option_detail.amount, unlock_amount)?;
        }
    }

    Ok(())
}

#[derive(Accounts)]
#[instruction(params: CloseOptionParams)]
pub struct CloseOption<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    // ✅ CRITICAL FIX: funding_account should match LOCKED asset, not premium asset
    #[account(
        mut,
        constraint = funding_account.mint == locked_custody.mint,
        has_one = owner
    )]
    pub funding_account: Box<Account<'info, TokenAccount>>,

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
        seeds = [b"pool",
                 params.pool_name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        seeds = [b"user", owner.key().as_ref()],
        bump,
    )]
    pub user: Box<Account<'info, User>>,

    // ✅ MOVE MINTS TO TOP
    #[account(mut)]
    pub custody_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub pay_custody_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub locked_custody_mint: Box<Account<'info, Mint>>,

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody_mint.key().as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>, // underlying price asset

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 pay_custody_mint.key().as_ref()],
        bump = pay_custody.bump
    )]
    pub pay_custody: Box<Account<'info, Custody>>, // premium payment asset

    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 locked_custody_mint.key().as_ref()],
        bump = locked_custody.bump
    )]
    pub locked_custody: Box<Account<'info, Custody>>, // locked asset (where refund comes from)

    // ✅ CRITICAL FIX: Transfer comes from locked custody token account
    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 locked_custody_mint.key().as_ref()],
        bump,
        constraint = locked_custody_token_account.mint == locked_custody_mint.key()
    )]
    pub locked_custody_token_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"option", owner.key().as_ref(),
            params.option_index.to_le_bytes().as_ref(),
            pool.key().as_ref(), custody.key().as_ref()],
        bump
    )]
    pub option_detail: Box<Account<'info, OptionDetail>>,

    #[account(
        init_if_needed,
        payer = owner,
        space = OptionDetail::LEN,
        seeds = [b"option", owner.key().as_ref(),
            params.option_index.to_le_bytes().as_ref(),
            pool.key().as_ref(), custody.key().as_ref(),
            b"closed"],
        bump
    )]
    pub closed_option_detail: Box<Account<'info, OptionDetail>>,

    /// CHECK: oracle for underlying asset
    #[account(constraint = custody_oracle_account.key() == custody.oracle)]
    pub custody_oracle_account: AccountInfo<'info>,

    /// CHECK: oracle for payment asset
    #[account(constraint = pay_custody_oracle_account.key() == pay_custody.oracle)]
    pub pay_custody_oracle_account: AccountInfo<'info>,

    /// CHECK: oracle for locked asset
    #[account(constraint = locked_oracle.key() == locked_custody.oracle)]
    pub locked_oracle: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
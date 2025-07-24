use anchor_lang::prelude::*;
use crate::errors::TradingError;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default)]
pub struct TpSlOrder {
    pub price: u64,           // Target price (scaled by 1e6)
    pub size_percent: u16,    // Percentage of position to close (basis points, max 10000 = 100%)
    pub receive_sol: bool,    // true = receive SOL, false = receive USDC
    pub is_active: bool,      // Whether this order is active
}

#[account]
#[derive(Default, Debug)]
pub struct TpSlOrderbook {
    // Identity
    pub owner: Pubkey,              // Position owner
    pub position: Pubkey,           // Associated position account
    pub contract_type: u8,          // 0 = Perp, 1 = Option
    
    // Orders (max 10 each)
    pub take_profit_orders: [TpSlOrder; 10],
    pub stop_loss_orders: [TpSlOrder; 10],
    
    // Counters
    pub active_tp_count: u8,        // Number of active TP orders
    pub active_sl_count: u8,        // Number of active SL orders
    pub total_tp_percent: u16,      // Total percentage allocated to TPs (basis points)
    pub total_sl_percent: u16,      // Total percentage allocated to SLs (basis points)
    
    // Execution tracking
    pub last_executed_tp_index: Option<u8>,  // Last executed TP order index
    pub last_executed_sl_index: Option<u8>,  // Last executed SL order index
    pub last_execution_time: i64,             // Last execution timestamp
    
    pub bump: u8,
}

impl TpSlOrderbook {
    pub const LEN: usize = 8 + std::mem::size_of::<TpSlOrderbook>();
    pub const MAX_ORDERS: usize = 10;
    
    pub fn initialize(
        &mut self,
        owner: Pubkey,
        position: Pubkey,
        contract_type: u8,
        bump: u8,
    ) -> Result<()> {
        self.owner = owner;
        self.position = position;
        self.contract_type = contract_type;
        self.bump = bump;
        self.active_tp_count = 0;
        self.active_sl_count = 0;
        self.total_tp_percent = 0;
        self.total_sl_percent = 0;
        self.last_execution_time = 0;
        self.last_executed_tp_index = None;
        self.last_executed_sl_index = None;
        
        // Initialize all orders as inactive
        for i in 0..Self::MAX_ORDERS {
            self.take_profit_orders[i] = TpSlOrder::default();
            self.stop_loss_orders[i] = TpSlOrder::default();
        }
        
        Ok(())
    }
    
    pub fn add_take_profit_order(
        &mut self,
        price: u64,
        size_percent: u16,
        receive_sol: bool,
    ) -> Result<usize> {
        require!(self.active_tp_count < Self::MAX_ORDERS as u8, TradingError::OrderbookFull);
        require!(size_percent > 0 && size_percent <= 10000, TradingError::InvalidAmount);
        
        // Find first inactive slot
        for i in 0..Self::MAX_ORDERS {
            if !self.take_profit_orders[i].is_active {
                self.take_profit_orders[i] = TpSlOrder {
                    price,
                    size_percent,
                    receive_sol,
                    is_active: true,
                };
                self.active_tp_count += 1;
                self.total_tp_percent += size_percent;
                return Ok(i);
            }
        }
        
        Err(TradingError::OrderbookFull.into())
    }
    
    pub fn add_stop_loss_order(
        &mut self,
        price: u64,
        size_percent: u16,
        receive_sol: bool,
    ) -> Result<usize> {
        require!(self.active_sl_count < Self::MAX_ORDERS as u8, TradingError::OrderbookFull);
        require!(size_percent > 0 && size_percent <= 10000, TradingError::InvalidAmount);
        
        // Find first inactive slot
        for i in 0..Self::MAX_ORDERS {
            if !self.stop_loss_orders[i].is_active {
                self.stop_loss_orders[i] = TpSlOrder {
                    price,
                    size_percent,
                    receive_sol,
                    is_active: true,
                };
                self.active_sl_count += 1;
                self.total_sl_percent += size_percent;
                return Ok(i);
            }
        }
        
        Err(TradingError::OrderbookFull.into())
    }
    
    pub fn remove_take_profit_order(&mut self, index: usize) -> Result<()> {
        require!(index < Self::MAX_ORDERS, TradingError::InvalidAmount);
        let order = &mut self.take_profit_orders[index];
        require!(order.is_active, TradingError::InvalidAmount);
        
        self.total_tp_percent -= order.size_percent;
        self.active_tp_count -= 1;
        *order = TpSlOrder::default();
        
        Ok(())
    }
    
    pub fn remove_stop_loss_order(&mut self, index: usize) -> Result<()> {
        require!(index < Self::MAX_ORDERS, TradingError::InvalidAmount);
        let order = &mut self.stop_loss_orders[index];
        require!(order.is_active, TradingError::InvalidAmount);
        
        self.total_sl_percent -= order.size_percent;
        self.active_sl_count -= 1;
        *order = TpSlOrder::default();
        
        Ok(())
    }
    
    pub fn update_take_profit_order(
        &mut self,
        index: usize,
        new_price: Option<u64>,
        new_size_percent: Option<u16>,
        new_receive_sol: Option<bool>,
    ) -> Result<()> {
        require!(index < Self::MAX_ORDERS, TradingError::InvalidAmount);
        let order = &mut self.take_profit_orders[index];
        require!(order.is_active, TradingError::InvalidAmount);
        
        if let Some(price) = new_price {
            order.price = price;
        }
        
        if let Some(size_percent) = new_size_percent {
            require!(size_percent > 0 && size_percent <= 10000, TradingError::InvalidAmount);
            let new_total = self.total_tp_percent - order.size_percent + size_percent;
            
            self.total_tp_percent = new_total;
            order.size_percent = size_percent;
        }
        
        if let Some(receive_sol) = new_receive_sol {
            order.receive_sol = receive_sol;
        }
        
        Ok(())
    }
    
    pub fn update_stop_loss_order(
        &mut self,
        index: usize,
        new_price: Option<u64>,
        new_size_percent: Option<u16>,
        new_receive_sol: Option<bool>,
    ) -> Result<()> {
        require!(index < Self::MAX_ORDERS, TradingError::InvalidAmount);
        let order = &mut self.stop_loss_orders[index];
        require!(order.is_active, TradingError::InvalidAmount);
        
        if let Some(price) = new_price {
            order.price = price;
        }
        
        if let Some(size_percent) = new_size_percent {
            require!(size_percent > 0 && size_percent <= 10000, TradingError::InvalidAmount);
            let new_total = self.total_sl_percent - order.size_percent + size_percent;
            
            self.total_sl_percent = new_total;
            order.size_percent = size_percent;
        }
        
        if let Some(receive_sol) = new_receive_sol {
            order.receive_sol = receive_sol;
        }
        
        Ok(())
    }
    
    pub fn mark_tp_executed(&mut self, index: usize, current_time: i64) -> Result<()> {
        require!(index < Self::MAX_ORDERS, TradingError::InvalidAmount);
        let order = &mut self.take_profit_orders[index];
        require!(order.is_active, TradingError::InvalidAmount);
        
        self.total_tp_percent -= order.size_percent;
        self.active_tp_count -= 1;
        *order = TpSlOrder::default();
        
        self.last_executed_tp_index = Some(index as u8);
        self.last_execution_time = current_time;
        
        Ok(())
    }
    
    pub fn mark_sl_executed(&mut self, index: usize, current_time: i64) -> Result<()> {
        require!(index < Self::MAX_ORDERS, TradingError::InvalidAmount);
        let order = &mut self.stop_loss_orders[index];
        require!(order.is_active, TradingError::InvalidAmount);
        
        self.total_sl_percent -= order.size_percent;
        self.active_sl_count -= 1;
        *order = TpSlOrder::default();
        
        self.last_executed_sl_index = Some(index as u8);
        self.last_execution_time = current_time;
        
        Ok(())
    }
    
    pub fn clear_all_orders(&mut self) -> Result<()> {
        for i in 0..Self::MAX_ORDERS {
            self.take_profit_orders[i] = TpSlOrder::default();
            self.stop_loss_orders[i] = TpSlOrder::default();
        }
        
        self.active_tp_count = 0;
        self.active_sl_count = 0;
        self.total_tp_percent = 0;
        self.total_sl_percent = 0;
        
        Ok(())
    }
}
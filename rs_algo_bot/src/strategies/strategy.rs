use crate::strategies;
use async_trait::async_trait;
use dyn_clone::DynClone;
use rs_algo_shared::error::Result;

use rs_algo_shared::models::market::{MarketHours, MarketSessions};
use rs_algo_shared::models::order::{self, Order, OrderType};
use rs_algo_shared::models::strategy::StrategyStats;
use rs_algo_shared::models::tick::InstrumentTick;
use rs_algo_shared::models::time_frame::TimeFrameType;
use rs_algo_shared::models::trade::*;
use rs_algo_shared::models::{strategy::*, trade};
use rs_algo_shared::scanner::candle::Candle;
use rs_algo_shared::scanner::instrument::*;
use std::cmp::Ordering;
use std::env;

#[async_trait(?Send)]
pub trait Strategy: DynClone {
    fn new(
        name: Option<&'static str>,
        time_frame: Option<&str>,
        higher_time_frame: Option<&str>,
        strategy_type: Option<StrategyType>,
    ) -> Result<Self>
    where
        Self: Sized;
    fn name(&self) -> &str;
    fn strategy_type(&self) -> &StrategyType;
    fn time_frame(&self) -> &TimeFrameType;
    fn higher_time_frame(&self) -> &Option<TimeFrameType>;
    fn entry_long(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        tick: &InstrumentTick,
    ) -> Position;
    fn exit_long(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        trade_in: &TradeIn,
        tick: &InstrumentTick,
    ) -> Position;
    fn entry_short(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        tick: &InstrumentTick,
    ) -> Position;
    fn exit_short(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        trade_in: &TradeIn,
        tick: &InstrumentTick,
    ) -> Position;
    fn trading_direction(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        market_hours: &MarketHours,
    ) -> &TradeDirection;
    fn is_long_strategy(&self) -> bool {
        match self.strategy_type() {
            StrategyType::OnlyLong
            | StrategyType::LongShort
            | StrategyType::OnlyLongMTF
            | StrategyType::LongShortMTF => true,
            _ => false,
        }
    }
    fn is_short_strategy(&self) -> bool {
        match self.strategy_type() {
            StrategyType::OnlyShort
            | StrategyType::LongShort
            | StrategyType::OnlyShortMTF
            | StrategyType::LongShortMTF => true,
            _ => false,
        }
    }
    async fn next(
        &mut self,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        trades_in: &Vec<TradeIn>,
        trades_out: &Vec<TradeOut>,
        orders: &Vec<Order>,
        tick: &InstrumentTick,
        market_hours: &MarketHours,
    ) -> (PositionResult, PositionResult) {
        let index = &instrument.data.len() - 1;
        let mut position_result = PositionResult::None;
        let mut order_position_result = PositionResult::None;
        let pending_orders = order::get_pending(orders);
        let trade_direction = &self
            .trading_direction(index, instrument, htf_instrument, market_hours)
            .clone();

        let open_positions = match trades_in.len().cmp(&trades_out.len()) {
            Ordering::Greater => true,
            _ => false,
        };

        order_position_result = self.pending_orders_activated(
            index,
            instrument,
            &pending_orders,
            trades_in,
            Some(tick),
            false,
        );

        if open_positions {
            let current_trade_fulfilled = match trades_in.last() {
                Some(trade) => trade.is_fulfilled(),
                None => true,
            };

            if current_trade_fulfilled {
                let current_trade_in = trades_in.last().unwrap();

                position_result = self.should_exit_position(
                    index,
                    instrument,
                    htf_instrument,
                    current_trade_in,
                    tick,
                );
            } else {
                log::warn!("Previous tradeIn no fulfilled");
            }
        }

        if !open_positions {
            let current_trade_fulfilled = match trades_out.last() {
                Some(trade) => trade.is_fulfilled(),
                None => true,
            };

            if current_trade_fulfilled {
                position_result = self.should_open_position(
                    index,
                    instrument,
                    htf_instrument,
                    orders,
                    trades_out,
                    trade_direction,
                    tick,
                );
            } else {
                log::warn!("Previous tradeOut no fulfilled");
            }
        }

        (position_result, order_position_result)
    }

    fn should_open_position(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        orders: &Vec<Order>,
        trades_out: &Vec<TradeOut>,
        trade_direction: &TradeDirection,
        tick: &InstrumentTick,
    ) -> PositionResult {
        let trade_size = env::var("ORDER_SIZE").unwrap().parse::<f64>().unwrap();

        let max_pending_orders = env::var("MAX_PENDING_ORDERS")
            .unwrap()
            .parse::<usize>()
            .unwrap();

        let overwrite_orders = env::var("OVERWRITE_ORDERS")
            .unwrap()
            .parse::<bool>()
            .unwrap();

        let trading_direction = env::var("TRADING_DIRECTION")
            .unwrap()
            .parse::<bool>()
            .unwrap();

        let pending_orders = order::get_pending(orders);
        let no_pending_orders = pending_orders.is_empty();
        let wait_for_new_trade = trade::wait_for_new_trade(index, instrument, trades_out);

        match !wait_for_new_trade && no_pending_orders {
            true => {
                if self.is_long_strategy() && (trade_direction.is_long() || !trading_direction) {
                    match self.entry_long(index, instrument, htf_instrument, tick) {
                        Position::MarketIn(order_types) => {
                            let trade_type = TradeType::MarketInLong;
                            let trade_in_result = trade::resolve_trade_in(
                                index,
                                trade_size,
                                instrument,
                                &trade_type,
                                None,
                                tick,
                            );

                            let prepared_orders = order_types.map(|orders| {
                                order::prepare_orders(index, instrument, &trade_type, &orders, tick)
                            });

                            let new_orders = match overwrite_orders {
                                true => prepared_orders,
                                false => match pending_orders.len().cmp(&0) {
                                    std::cmp::Ordering::Equal => prepared_orders,
                                    _ => None,
                                },
                            };

                            log::info!("New Position: {:?}", trade_type);

                            PositionResult::MarketIn(trade_in_result, new_orders)
                        }
                        Position::Order(order_types) => {
                            let trade_type = TradeType::OrderInLong;

                            let prepared_orders = order::prepare_orders(
                                index,
                                instrument,
                                &trade_type,
                                &order_types,
                                tick,
                            );

                            let new_orders = match overwrite_orders {
                                true => prepared_orders,
                                false => match pending_orders.len().cmp(&0) {
                                    std::cmp::Ordering::Equal => prepared_orders,
                                    _ => vec![],
                                },
                            };

                            log_created_orders(&new_orders);

                            PositionResult::PendingOrder(new_orders)
                        }
                        _ => PositionResult::None,
                    }
                } else if self.is_short_strategy()
                    && (trade_direction.is_short() || !trading_direction)
                {
                    match self.entry_short(index, instrument, htf_instrument, tick) {
                        Position::MarketIn(order_types) => {
                            let trade_type = TradeType::MarketInShort;

                            let trade_in_result = trade::resolve_trade_in(
                                index,
                                trade_size,
                                instrument,
                                &trade_type,
                                None,
                                tick,
                            );

                            let prepared_orders = order_types.map(|orders| {
                                order::prepare_orders(index, instrument, &trade_type, &orders, tick)
                            });

                            let new_orders = match overwrite_orders {
                                true => prepared_orders,
                                false => match pending_orders.len().cmp(&0) {
                                    std::cmp::Ordering::Equal => prepared_orders,
                                    _ => None,
                                },
                            };

                            log::info!("New Position: {:?}", trade_type);

                            PositionResult::MarketIn(trade_in_result, new_orders)
                        }
                        Position::Order(order_types) => {
                            let trade_type = TradeType::OrderInShort;

                            let prepared_orders = order::prepare_orders(
                                index,
                                instrument,
                                &trade_type,
                                &order_types,
                                tick,
                            );

                            let new_orders = match overwrite_orders {
                                true => prepared_orders,
                                false => match pending_orders.len().cmp(&0) {
                                    std::cmp::Ordering::Equal => prepared_orders,
                                    _ => vec![],
                                },
                            };

                            log_created_orders(&new_orders);

                            PositionResult::PendingOrder(new_orders)
                        }
                        _ => PositionResult::None,
                    }
                } else {
                    PositionResult::None
                }
            }
            false => PositionResult::None,
        }
    }

    fn should_exit_position(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        trade_in: &TradeIn,
        tick: &InstrumentTick,
    ) -> PositionResult {
        let wait_for_closing_trade = trade::wait_for_closing_trade(index, instrument, trade_in);

        match wait_for_closing_trade {
            true => match trade_in.trade_type.is_long_entry() {
                true => match self.exit_long(index, instrument, htf_instrument, trade_in, tick) {
                    Position::MarketOut(_) => {
                        let trade_type = TradeType::MarketOutLong;
                        let trade_out_result = trade::resolve_trade_out(
                            index,
                            instrument,
                            trade_in,
                            &trade_type,
                            None,
                            tick,
                        );
                        PositionResult::MarketOut(trade_out_result)
                    }
                    Position::Order(order_types) => {
                        let trade_type = TradeType::OrderOutLong;
                        let orders = order::prepare_orders(
                            index,
                            instrument,
                            &trade_type,
                            &order_types,
                            tick,
                        );
                        PositionResult::PendingOrder(orders)
                    }
                    _ => PositionResult::None,
                },
                false => match self.exit_short(index, instrument, htf_instrument, trade_in, tick) {
                    Position::MarketOut(_) => {
                        let trade_type = TradeType::MarketOutShort;
                        let trade_out_result = trade::resolve_trade_out(
                            index,
                            instrument,
                            trade_in,
                            &trade_type,
                            None,
                            tick,
                        );

                        PositionResult::MarketOut(trade_out_result)
                    }
                    Position::Order(order_types) => {
                        let trade_type = TradeType::OrderOutShort;
                        let orders = order::prepare_orders(
                            index,
                            instrument,
                            &trade_type,
                            &order_types,
                            tick,
                        );
                        PositionResult::PendingOrder(orders)
                    }
                    _ => PositionResult::None,
                },
            },
            false => PositionResult::None,
        }
    }

    fn pending_orders_activated(
        &mut self,
        index: usize,
        instrument: &Instrument,
        pending_orders: &Vec<Order>,
        trades_in: &Vec<TradeIn>,
        tick: Option<&InstrumentTick>,
        use_tick_price: bool,
    ) -> PositionResult {
        let tick = tick.expect("Failed to unwrap Tick: None");
        match order::resolve_active_orders(index, instrument, pending_orders, tick, use_tick_price)
        {
            Position::MarketInOrder(mut order) => {
                let order_size = order.size();
                let trade_type = order.to_trade_type();

                let trade_in_result = trade::resolve_trade_in(
                    index,
                    order_size,
                    instrument,
                    &trade_type,
                    Some(&order),
                    tick,
                );

                let trade_id = match &trade_in_result {
                    TradeResult::TradeIn(trade_in) => trade_in.id,
                    _ => 0,
                };

                log::info!("Order activated: {:?} ", order.order_type);
                // order.set_status(order::OrderStatus::Fulfilled);
                order.set_trade_id(trade_id);
                PositionResult::MarketInOrder(trade_in_result, order)
            }
            Position::MarketOutOrder(order) => {
                let trade_type = order.to_trade_type();

                let trade_out_result = match trades_in.iter().filter(|x| x.is_fulfilled()).last() {
                    Some(trade_in) => trade::resolve_trade_out(
                        index,
                        instrument,
                        trade_in,
                        &trade_type,
                        Some(&order),
                        tick,
                    ),
                    None => TradeResult::None,
                };
                log::info!("Order activated: {:?} ", order.order_type);
                // order.set_status(order::OrderStatus::Fulfilled);
                PositionResult::MarketOutOrder(trade_out_result, order)
            }
            _ => PositionResult::None,
        }
    }

    fn there_are_funds(&mut self, trades_out: &Vec<TradeOut>) -> bool {
        let profit: f64 = trades_out.iter().map(|trade| trade.profit_per).sum();
        profit > -90.
    }

    fn update_stats(
        &self,
        instrument: &Instrument,
        trades_in: &Vec<TradeIn>,
        trades_out: &Vec<TradeOut>,
    ) -> StrategyStats {
        let equity = env::var("EQUITY").unwrap().parse::<f64>().unwrap();
        let commission = env::var("COMMISSION").unwrap().parse::<f64>().unwrap();
        calculate_strategy_stats(instrument, trades_in, trades_out, equity, commission)
    }

    fn update_trade_stats(
        &self,
        trade_in: &TradeIn,
        trade_out: &TradeOut,
        data: &Vec<Candle>,
    ) -> TradeOut {
        calculate_trade_stats(trade_in, trade_out, data)
    }
}

pub fn set_strategy(
    strategy_name: &str,
    time_frame: &str,
    higher_time_frame: Option<&str>,
    strategy_type: StrategyType,
) -> Box<dyn Strategy> {
    let strategies: Vec<Box<dyn Strategy>> = vec![
        Box::new(
            strategies::bollinger_bands_reversals_buy_exit::BollingerBandsReversals::new(
                Some("BB_Reversals_Backtest_510"),
                Some(time_frame),
                higher_time_frame,
                Some(strategy_type.clone()),
            )
            .unwrap(),
        ),
        Box::new(
            strategies::bollinger_bands_reversals_buy_exit::BollingerBandsReversals::new(
                Some("BB_Reversals_Backtest_NoTd"),
                Some(time_frame),
                higher_time_frame,
                Some(strategy_type.clone()),
            )
            .unwrap(),
        ),
    ];

    let mut strategy = strategies[0].clone();
    let mut found = false;
    for stra in strategies.iter() {
        if strategy_name == stra.name() {
            strategy = stra.clone();
            found = true
        }
    }
    if found {
        log::info!("Using strategy {}", strategy.name());
    } else {
        panic!("Strategy {} not found!", strategy_name);
    }

    strategy
}

fn log_created_orders(orders: &[Order]) {
    let orders_created: Vec<&OrderType> =
        orders
            .iter()
            .map(|order| &order.order_type)
            .fold(Vec::new(), |mut acc, order_type| {
                if !acc.contains(&order_type) {
                    acc.push(order_type);
                }
                acc
            });

    if orders_created.len() > 0 {
        log::info!("Orders created: {:?}", orders_created);
    }
}

dyn_clone::clone_trait_object!(Strategy);

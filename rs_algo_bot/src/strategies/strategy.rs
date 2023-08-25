use super::stats::*;

use crate::strategies;
use async_trait::async_trait;
use chrono::Local;
use dyn_clone::DynClone;
use rs_algo_shared::error::Result;

use rs_algo_shared::helpers::date;
use rs_algo_shared::models::order::{self, Order};
use rs_algo_shared::models::pricing::Pricing;
use rs_algo_shared::models::strategy::StrategyStats;
use rs_algo_shared::models::time_frame::TimeFrameType;
use rs_algo_shared::models::{mode, trade::*};
use rs_algo_shared::models::{strategy::*, trade};
use rs_algo_shared::scanner::candle::Candle;
use rs_algo_shared::scanner::instrument::*;
use std::cmp::Ordering;
use std::env;

#[async_trait(?Send)]
pub trait Strategy: DynClone {
    fn new(
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
        pricing: &Pricing,
    ) -> Position;
    fn exit_long(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        trade_in: &TradeIn,
        pricing: &Pricing,
    ) -> Position;
    fn entry_short(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        pricing: &Pricing,
    ) -> Position;
    fn exit_short(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        trade_in: &TradeIn,
        pricing: &Pricing,
    ) -> Position;
    fn trading_direction(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
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
    async fn tick(
        &mut self,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        trades_in: &Vec<TradeIn>,
        trades_out: &Vec<TradeOut>,
        orders: &Vec<Order>,
        pricing: &Pricing,
    ) -> (PositionResult, PositionResult) {
        let index = &instrument.data.len() - 1;
        let mut position_result = PositionResult::None;
        let mut order_position_result = PositionResult::None;
        let pending_orders = order::get_pending(orders);
        let trading_direction = &self
            .trading_direction(index, instrument, htf_instrument)
            .clone();

        let open_positions = match trades_in.len().cmp(&trades_out.len()) {
            Ordering::Greater => true,
            _ => false,
        };

        order_position_result =
            self.resolve_pending_orders(index, instrument, pricing, &pending_orders, trades_in);

        if open_positions {
            let trade_in = trades_in.last().unwrap();
            position_result =
                self.resolve_exit_position(index, instrument, htf_instrument, pricing, &trade_in);
        }

        if !open_positions && self.there_are_funds(trades_out) {
            position_result = self.resolve_entry_position(
                index,
                instrument,
                htf_instrument,
                pricing,
                orders,
                trades_out,
                trading_direction,
            );
        }

        (position_result, order_position_result)
    }

    fn resolve_entry_position(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        pricing: &Pricing,
        orders: &Vec<Order>,
        trades_out: &Vec<TradeOut>,
        trading_direction: &TradeDirection,
    ) -> PositionResult {
        let pending_orders = order::get_pending(orders);
        let trade_size = env::var("ORDER_SIZE").unwrap().parse::<f64>().unwrap();

        let overwrite_orders = env::var("OVERWRITE_ORDERS")
            .unwrap()
            .parse::<bool>()
            .unwrap();

        let wait_for_opening_trade = self.wait_for_opening_trade(index, instrument, trades_out);

        match wait_for_opening_trade {
            true => match trading_direction.is_long() {
                true => match self.is_long_strategy() {
                    true => match self.entry_long(index, instrument, htf_instrument, pricing) {
                        Position::MarketIn(order_types) => {
                            let trade_type = TradeType::MarketInLong;
                            let trade_in_result = trade::resolve_trade_in(
                                index,
                                trade_size,
                                instrument,
                                pricing,
                                &trade_type,
                                None,
                            );

                            let prepared_orders = order_types.map(|orders| {
                                order::prepare_orders(
                                    index,
                                    instrument,
                                    pricing,
                                    &trade_type,
                                    &orders,
                                )
                            });

                            let new_orders = match overwrite_orders {
                                true => prepared_orders,
                                false => match pending_orders.len().cmp(&0) {
                                    std::cmp::Ordering::Equal => prepared_orders,
                                    _ => None,
                                },
                            };

                            PositionResult::MarketIn(trade_in_result, new_orders)
                        }
                        Position::Order(order_types) => {
                            let trade_type = TradeType::OrderInLong;

                            let prepared_orders = order::prepare_orders(
                                index,
                                instrument,
                                pricing,
                                &trade_type,
                                &order_types,
                            );

                            let new_orders = match overwrite_orders {
                                true => prepared_orders,
                                false => match pending_orders.len().cmp(&0) {
                                    std::cmp::Ordering::Equal => prepared_orders,
                                    _ => vec![],
                                },
                            };

                            PositionResult::PendingOrder(new_orders)
                        }
                        _ => PositionResult::None,
                    },
                    false => PositionResult::None,

                    _ => PositionResult::None,
                },
                false => match self.is_short_strategy() {
                    true => match self.entry_short(index, instrument, htf_instrument, pricing) {
                        Position::MarketIn(order_types) => {
                            let trade_type = TradeType::MarketInShort;

                            let trade_in_result = trade::resolve_trade_in(
                                index,
                                trade_size,
                                instrument,
                                pricing,
                                &trade_type,
                                None,
                            );

                            let prepared_orders = order_types.map(|orders| {
                                order::prepare_orders(
                                    index,
                                    instrument,
                                    pricing,
                                    &trade_type,
                                    &orders,
                                )
                            });

                            let new_orders = match overwrite_orders {
                                true => prepared_orders,
                                false => match pending_orders.len().cmp(&0) {
                                    std::cmp::Ordering::Equal => prepared_orders,
                                    _ => None,
                                },
                            };

                            PositionResult::MarketIn(trade_in_result, new_orders)
                        }
                        Position::Order(order_types) => {
                            let trade_type = TradeType::OrderInShort;

                            let prepared_orders = order::prepare_orders(
                                index,
                                instrument,
                                pricing,
                                &trade_type,
                                &order_types,
                            );

                            let new_orders = match overwrite_orders {
                                true => prepared_orders,
                                false => match pending_orders.len().cmp(&0) {
                                    std::cmp::Ordering::Equal => prepared_orders,
                                    _ => vec![],
                                },
                            };

                            PositionResult::PendingOrder(new_orders)
                        }
                        _ => PositionResult::None,
                    },
                    false => PositionResult::None,
                    _ => PositionResult::None,
                },
                false => PositionResult::None,
            },
            false => PositionResult::None,
        }
    }

    fn resolve_exit_position(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        pricing: &Pricing,
        trade_in: &TradeIn,
    ) -> PositionResult {
        let wait_for_closing_trade = self.wait_for_closing_trade(index, instrument, trade_in);

        match wait_for_closing_trade {
            true => match trade_in.trade_type.is_long_entry() {
                true => {
                    match self.exit_long(index, instrument, htf_instrument, trade_in, pricing) {
                        Position::MarketOut(_) => {
                            let trade_type = TradeType::MarketOutLong;
                            let trade_out_result = trade::resolve_trade_out(
                                index,
                                instrument,
                                pricing,
                                trade_in,
                                &trade_type,
                                None,
                            );
                            PositionResult::MarketOut(trade_out_result)
                        }
                        Position::Order(order_types) => {
                            let trade_type = TradeType::OrderOutLong;
                            let orders = order::prepare_orders(
                                index,
                                instrument,
                                pricing,
                                &trade_type,
                                &order_types,
                            );
                            PositionResult::PendingOrder(orders)
                        }
                        _ => PositionResult::None,
                    }
                }
                false => {
                    match self.exit_short(index, instrument, htf_instrument, trade_in, pricing) {
                        Position::MarketOut(_) => {
                            let trade_type = TradeType::MarketOutShort;
                            let trade_out_result = trade::resolve_trade_out(
                                index,
                                instrument,
                                pricing,
                                trade_in,
                                &trade_type,
                                None,
                            );

                            PositionResult::MarketOut(trade_out_result)
                        }
                        Position::Order(order_types) => {
                            let trade_type = TradeType::OrderOutShort;
                            let orders = order::prepare_orders(
                                index,
                                instrument,
                                pricing,
                                &trade_type,
                                &order_types,
                            );
                            PositionResult::PendingOrder(orders)
                        }
                        _ => PositionResult::None,
                    }
                }
            },
            false => PositionResult::None,
        }
    }

    fn resolve_pending_orders(
        &mut self,
        index: usize,
        instrument: &Instrument,
        pricing: &Pricing,
        pending_orders: &Vec<Order>,
        trades_in: &Vec<TradeIn>,
    ) -> PositionResult {
        match order::resolve_active_orders(index, instrument, pending_orders, pricing) {
            Position::MarketInOrder(mut order) => {
                let order_size = order.size();
                let trade_type = order.to_trade_type();
                let trade_in_result = trade::resolve_trade_in(
                    index,
                    order_size,
                    instrument,
                    pricing,
                    &trade_type,
                    Some(&order),
                );

                let trade_id = match &trade_in_result {
                    TradeResult::TradeIn(trade_in) => trade_in.id,
                    _ => 0,
                };
                log::info!("Order: Entry");

                order.set_trade_id(trade_id);
                PositionResult::MarketInOrder(trade_in_result, order)
            }
            Position::MarketOutOrder(order) => {
                let trade_type = order.to_trade_type();
                let trade_out_result = match trades_in.last() {
                    Some(trade_in) => trade::resolve_trade_out(
                        index,
                        instrument,
                        pricing,
                        trade_in,
                        &trade_type,
                        Some(&order),
                    ),
                    None => TradeResult::None,
                };

                log::info!("Order: Exit");

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
        calculate_stats(instrument, trades_in, trades_out, equity, commission)
    }

    fn update_trade_stats(
        &self,
        trade_in: &TradeIn,
        trade_out: &TradeOut,
        data: &Vec<Candle>,
        pricing: &Pricing,
    ) -> TradeOut {
        calculate_trade_stats(trade_in, trade_out, data, pricing)
    }

    fn wait_for_opening_trade(
        &self,
        index: usize,
        instrument: &Instrument,
        trades_out: &Vec<TradeOut>,
    ) -> bool {
        let wait_for_new_entry = env::var("WAIT_FOR_NEW_ENTRY")
            .unwrap()
            .parse::<bool>()
            .unwrap();

        match wait_for_new_entry {
            true => {
                let execution_mode = mode::from_str(&env::var("EXECUTION_MODE").unwrap());

                let candles_until_new_operation = env::var("CANDLES_UNTIL_NEW_ENTRY")
                    .unwrap()
                    .parse::<i64>()
                    .unwrap();

                let time_frame = instrument.time_frame();

                let current_date = match execution_mode.is_back_test() {
                    true => instrument.data().get(index).unwrap().date(),
                    false => Local::now(),
                };

                match trades_out.last() {
                    Some(trade_out) => {
                        let next_entry_date = match instrument.time_frame().is_minutely_time_frame()
                        {
                            true => {
                                date::from_dbtime(&trade_out.date_out)
                                    + date::Duration::minutes(
                                        candles_until_new_operation * time_frame.to_minutes(),
                                    )
                            }
                            false => {
                                date::from_dbtime(&trade_out.date_out)
                                    + date::Duration::hours(
                                        candles_until_new_operation * time_frame.to_hours(),
                                    )
                            }
                        };
                        next_entry_date <= current_date
                    }
                    None => true,
                }
            }
            false => true,
        }
    }

    fn wait_for_closing_trade(
        &self,
        index: usize,
        instrument: &Instrument,
        trade_in: &TradeIn,
    ) -> bool {
        let wait_for_new_exit = env::var("WAIT_FOR_NEW_EXIT")
            .unwrap()
            .parse::<bool>()
            .unwrap();

        match wait_for_new_exit {
            true => {
                let execution_mode = mode::from_str(&env::var("EXECUTION_MODE").unwrap());

                let candles_until_new_operation = env::var("CANDLES_UNTIL_NEW_ENTRY")
                    .unwrap()
                    .parse::<i64>()
                    .unwrap();

                let time_frame = instrument.time_frame();

                let current_date = match execution_mode.is_back_test() {
                    true => instrument.data().get(index).unwrap().date(),
                    false => Local::now(),
                };

                let next_entry_date = match instrument.time_frame().is_minutely_time_frame() {
                    true => {
                        date::from_dbtime(&trade_in.date_in)
                            + date::Duration::minutes(
                                candles_until_new_operation * time_frame.to_minutes(),
                            )
                    }
                    false => {
                        date::from_dbtime(&trade_in.date_in)
                            + date::Duration::hours(
                                candles_until_new_operation * time_frame.to_hours(),
                            )
                    }
                };
                next_entry_date <= current_date
            }
            false => true,
        }
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
            strategies::bollinger_bands_reversals::BollingerBandsReversals::new(
                Some(time_frame),
                higher_time_frame,
                Some(strategy_type.clone()),
            )
            .unwrap(),
        ),
        Box::new(
            strategies::bollinger_bands_middle_band::BollingerBandsMiddleBand::new(
                Some(time_frame),
                higher_time_frame,
                Some(strategy_type.clone()),
            )
            .unwrap(),
        ),
        Box::new(
            strategies::ema_scalping::EmaScalping::new(
                Some(time_frame),
                higher_time_frame,
                Some(strategy_type.clone()),
            )
            .unwrap(),
        ),
        Box::new(
            strategies::ema_scalping2::EmaScalping2::new(
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
        panic!("Strategy not found!");
    }

    strategy
}

dyn_clone::clone_trait_object!(Strategy);

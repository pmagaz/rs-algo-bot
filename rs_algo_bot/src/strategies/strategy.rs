use super::stats::*;

use crate::strategies;
use async_trait::async_trait;
use dyn_clone::DynClone;
use rs_algo_shared::error::Result;

use rs_algo_shared::models::order::{self, Order, OrderType};
use rs_algo_shared::models::pricing::Pricing;
use rs_algo_shared::models::strategy::StrategyStats;
use rs_algo_shared::models::strategy::*;
use rs_algo_shared::models::time_frame::TimeFrameType;
use rs_algo_shared::models::trade::*;
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
        pricing: &Pricing,
    ) -> Position;
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
        let pending_orders = order::get_pending(&orders);

        let open_positions = match trades_in.len().cmp(&trades_out.len()) {
            Ordering::Greater => true,
            _ => false,
        };

        order_position_result =
            self.resolve_pending_orders(index, instrument, pricing, &pending_orders, &trades_in);

        if open_positions {
            let trade_in = trades_in.last().unwrap().to_owned();
            position_result =
                self.resolve_exit_position(index, instrument, htf_instrument, pricing, &trade_in);
        }

        if !open_positions && self.there_are_funds(&trades_out) {
            position_result =
                self.resolve_entry_position(index, instrument, htf_instrument, &orders, pricing);
        }

        log::info!(
            "Position Result {:?}",
            (open_positions, &position_result, &order_position_result)
        );
        (position_result, order_position_result)
    }
    fn resolve_entry_position(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        orders: &Vec<Order>,
        pricing: &Pricing,
    ) -> PositionResult {
        let mut long_entry: bool = false;
        let mut short_entry: bool = false;
        let pending_orders = order::get_pending(orders);
        let overwrite_orders = env::var("OVERWRITE_ORDERS")
            .unwrap()
            .parse::<bool>()
            .unwrap();

        let trade_size = env::var("ORDER_SIZE").unwrap().parse::<f64>().unwrap();

        let entry_long = match self.strategy_type() {
            StrategyType::OnlyLong
            | StrategyType::LongShort
            | StrategyType::OnlyLongMTF
            | StrategyType::LongShortMTF => {
                match self.entry_long(index, instrument, htf_instrument, pricing) {
                    Position::MarketIn(order_types) => {
                        let trade_type = TradeType::MarketInLong;
                        let trade_in_result = resolve_trade_in(
                            index,
                            trade_size,
                            instrument,
                            pricing,
                            &trade_type,
                            None,
                        );

                        let prepared_orders = match order_types {
                            Some(orders) => {
                                long_entry = true;
                                short_entry = false;
                                Some(order::prepare_orders(
                                    index,
                                    instrument,
                                    pricing,
                                    &trade_type,
                                    &orders,
                                ))
                            }
                            None => None,
                        };

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

                        if new_orders.len() > 0 {
                            long_entry = true;
                            short_entry = false;
                        }

                        PositionResult::PendingOrder(new_orders)
                    }
                    _ => PositionResult::None,
                }
            }
            _ => PositionResult::None,
        };

        let entry_short = match self.strategy_type() {
            StrategyType::OnlyShort
            | StrategyType::LongShort
            | StrategyType::OnlyShortMTF
            | StrategyType::LongShortMTF => {
                match self.entry_short(index, instrument, htf_instrument, pricing) {
                    Position::MarketIn(order_types) => {
                        let trade_type = TradeType::MarketInShort;
                        let trade_in_result = resolve_trade_in(
                            index,
                            trade_size,
                            instrument,
                            pricing,
                            &trade_type,
                            None,
                        );

                        let prepared_orders = match order_types {
                            Some(orders) => {
                                short_entry = true;
                                long_entry = false;
                                Some(order::prepare_orders(
                                    index,
                                    instrument,
                                    pricing,
                                    &trade_type,
                                    &orders,
                                ))
                            }
                            None => None,
                        };

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

                        if new_orders.len() > 0 {
                            short_entry = true;
                            long_entry = false;
                        }

                        PositionResult::PendingOrder(new_orders)
                    }
                    _ => PositionResult::None,
                }
            }
            _ => PositionResult::None,
        };

        log::info!("Entry: {:?}", (long_entry, short_entry));

        if long_entry && !short_entry {
            entry_long
        } else if !long_entry && short_entry {
            entry_short
        } else {
            PositionResult::None
        }
    }

    fn resolve_exit_position(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
        pricing: &Pricing,
        trade_in: &TradeIn,
        //exit_type: &TradeType,
    ) -> PositionResult {
        let mut long_exit: bool = false;
        let mut short_exit: bool = false;

        let exit_long = match self.strategy_type() {
            StrategyType::OnlyLong
            | StrategyType::LongShort
            | StrategyType::OnlyLongMTF
            | StrategyType::LongShortMTF => {
                match self.exit_long(index, instrument, htf_instrument, trade_in, pricing) {
                    Position::MarketOut(_) => {
                        let trade_type = TradeType::MarketOutLong;
                        long_exit = true;
                        short_exit = false;
                        let trade_out_result = resolve_trade_out(
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
                        long_exit = true;
                        short_exit = false;
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
            _ => PositionResult::None,
        };

        let exit_short = match self.strategy_type() {
            StrategyType::OnlyShort
            | StrategyType::LongShort
            | StrategyType::OnlyShortMTF
            | StrategyType::LongShortMTF => {
                match self.exit_short(index, instrument, htf_instrument, pricing) {
                    Position::MarketOut(_) => {
                        let trade_type = TradeType::MarketOutShort;
                        short_exit = true;
                        long_exit = false;
                        let trade_out_result = resolve_trade_out(
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
                        short_exit = true;
                        long_exit = false;
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
            _ => PositionResult::None,
        };

        if long_exit && !short_exit {
            exit_long
        } else if !long_exit && short_exit {
            exit_short
        } else {
            PositionResult::None
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
                let trade_in_result = resolve_trade_in(
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

                order.set_trade_id(trade_id);
                PositionResult::MarketInOrder(trade_in_result, order)
            }
            Position::MarketOutOrder(order) => {
                let trade_type = order.to_trade_type();
                let trade_out_result = match trades_in.last() {
                    Some(trade_in) => resolve_trade_out(
                        index,
                        instrument,
                        pricing,
                        trade_in,
                        &trade_type,
                        Some(&order),
                    ),
                    None => TradeResult::None,
                };

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
}

pub fn set_strategy(strategy_name: &str) -> Box<dyn Strategy> {
    let strategies: Vec<Box<dyn Strategy>> = vec![Box::new(
        strategies::bollinger_bands_reversals::BollingerBandsReversals::new(None, None, None)
            .unwrap(),
    )];

    let mut strategy = strategies[0].clone();
    for stra in strategies.iter() {
        if strategy_name == stra.name() {
            strategy = stra.clone();
        }
    }

    log::info!("Using strategy {}", strategy.name());

    strategy
}

dyn_clone::clone_trait_object!(Strategy);

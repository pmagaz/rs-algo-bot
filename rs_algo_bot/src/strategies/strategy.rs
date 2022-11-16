use super::stats::*;

use crate::strategies;
use async_trait::async_trait;
use dyn_clone::DynClone;
use rs_algo_shared::error::Result;

use rs_algo_shared::models::stop_loss::*;
use rs_algo_shared::models::strategy::*;
use rs_algo_shared::models::trade::*;
use rs_algo_shared::scanner::instrument::*;
use std::cmp::Ordering;
use std::env;

#[async_trait(?Send)]
pub trait Strategy: DynClone {
    fn new() -> Result<Self>
    where
        Self: Sized;
    fn name(&self) -> &str;
    fn strategy_type(&self) -> &StrategyType;
    fn entry_long(
        &mut self,
        index: usize,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
    ) -> bool;
    fn exit_long(
        &mut self,
        index: usize,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
    ) -> bool;
    fn entry_short(
        &mut self,
        index: usize,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
    ) -> bool;
    fn exit_short(
        &mut self,
        index: usize,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
    ) -> bool;
    async fn tick(
        &mut self,
        instrument: &Instrument,
        higher_tf_instrument: &HigherTMInstrument,
        trades_in: &Vec<TradeIn>,
        trades_out: &Vec<TradeOut>,
    ) -> (TradeResult, TradeResult) {
        let data = &instrument.data;
        let index = data.len() - 1;
        let mut trade_in_result = TradeResult::None;
        let mut trade_out_result = TradeResult::None;

        let open_positions = match trades_in.len().cmp(&trades_out.len()) {
            Ordering::Greater => true,
            _ => false,
        };

        let order_size = env::var("ORDER_SIZE").unwrap().parse::<f64>().unwrap();

        let _start_date = match data.first().map(|x| x.date) {
            Some(date) => date.to_string(),
            None => "".to_string(),
        };

        if open_positions {
            let trade_in = trades_in.last().unwrap().to_owned();
            trade_out_result =
                self.market_out_fn(index, instrument, higher_tf_instrument, trade_in);
        }

        if !open_positions && self.there_are_funds(trades_out) {
            trade_in_result =
                self.market_in_fn(index, instrument, higher_tf_instrument, order_size);
        }
        (trade_out_result, trade_in_result)
    }
    fn market_in_fn(
        &mut self,
        index: usize,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
        order_size: f64,
    ) -> TradeResult {
        let entry_type: TradeType;

        if self.entry_long(index, instrument, upper_tf_instrument) {
            entry_type = TradeType::EntryLong
        } else if self.entry_short(index, instrument, upper_tf_instrument) {
            entry_type = TradeType::EntryShort
        } else {
            entry_type = TradeType::None
        }

        let stop_loss = self.stop_loss();

        resolve_trade_in(index, order_size, instrument, entry_type, stop_loss)
    }

    fn market_out_fn(
        &mut self,
        index: usize,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
        mut trade_in: TradeIn,
    ) -> TradeResult {
        let exit_type: TradeType;

        let stop_loss = self.stop_loss();

        if stop_loss.stop_type != StopLossType::Atr
            && stop_loss.stop_type != StopLossType::Percentage
        {
            trade_in.stop_loss = update_stop_loss_values(
                &trade_in.stop_loss,
                stop_loss.stop_type.to_owned(),
                stop_loss.price,
            );
        }

        if self.exit_long(index, instrument, upper_tf_instrument) {
            exit_type = TradeType::ExitLong
        } else if self.exit_short(index, instrument, upper_tf_instrument) {
            exit_type = TradeType::ExitShort
        } else {
            exit_type = TradeType::None
        }
        let _stop_loss = true;

        resolve_trade_out(index, instrument, trade_in, exit_type)
    }
    fn stop_loss(&self) -> &StopLoss;
    fn update_stop_loss(&mut self, stop_type: StopLossType, price: f64) -> &StopLoss;
    fn stop_loss_exit(&mut self, stop_type: StopLossType, price: f64) -> bool {
        let stop_loss = self.stop_loss();
        update_stop_loss_values(stop_loss, stop_type, price);
        true
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
        equity: f64,
        commission: f64,
    ) -> StrategyStats {
        calculate_stats(instrument, trades_in, trades_out, equity, commission)
    }
}

pub fn set_strategy(strategy_name: &str) -> Box<dyn Strategy> {
    let strategies: Vec<Box<dyn Strategy>> = vec![
        Box::new(strategies::stoch::Stoch::new().unwrap()),
        Box::new(
            strategies::bollinger_bands_reversals2_mt_macd::MutiTimeFrameBollingerBands::new()
                .unwrap(),
        ),
    ];

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

use super::stats::*;

use crate::strategies;
use async_trait::async_trait;
use dyn_clone::DynClone;
use rs_algo_shared::error::Result;

use rs_algo_shared::models::strategy::StrategyStats;
//use crate::strategies::stats::StrategyStats;
use rs_algo_shared::models::stop_loss::*;
use rs_algo_shared::models::strategy::*;

use rs_algo_shared::models::trade::*;
use rs_algo_shared::scanner::candle::Candle;
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
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
    ) -> bool;
    fn exit_long(
        &mut self,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
    ) -> bool;
    fn entry_short(
        &mut self,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
    ) -> bool;
    fn exit_short(
        &mut self,
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
            trade_out_result = self.market_out_fn(instrument, higher_tf_instrument, trade_in);
        }

        if !open_positions && self.there_are_funds(trades_out) {
            trade_in_result = self.market_in_fn(instrument, higher_tf_instrument, order_size);
        }
        (trade_out_result, trade_in_result)
    }
    fn market_in_fn(
        &mut self,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
        order_size: f64,
    ) -> TradeResult {
        let index = instrument.data().len() - 1;
        let entry_type: TradeType;
        if self.entry_long(instrument, upper_tf_instrument) {
            entry_type = TradeType::EntryLong
        } else if self.entry_short(instrument, upper_tf_instrument) {
            entry_type = TradeType::EntryShort
        } else {
            entry_type = TradeType::None
        }

        let stop_loss = self.stop_loss();

        resolve_bot_trade_in(index, order_size, instrument, entry_type, stop_loss)
    }

    fn market_out_fn(
        &mut self,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
        trade_in: TradeIn,
    ) -> TradeResult {
        let exit_type: TradeType;
        let index = instrument.data().len() - 1;
        //let stop_loss = self.stop_loss();

        // if stop_loss.stop_type != StopLossType::Atr
        //     && stop_loss.stop_type != StopLossType::Percentage
        // {
        //     trade_in.stop_loss = update_stop_loss_values(
        //         &trade_in.stop_loss,
        //         stop_loss.stop_type.to_owned(),
        //         stop_loss.price,
        //     );
        // }

        if self.exit_long(instrument, upper_tf_instrument) {
            exit_type = TradeType::ExitLong
        } else if self.exit_short(instrument, upper_tf_instrument) {
            exit_type = TradeType::ExitShort
        } else {
            exit_type = TradeType::None
        }
        let _stop_loss = true;

        resolve_bot_trade_out(index, instrument, trade_in, exit_type)
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
    ) -> TradeOut {
        calculate_trade_stats(trade_in, trade_out, data)
    }
}

pub fn set_strategy(strategy_name: &str) -> Box<dyn Strategy> {
    let strategies: Vec<Box<dyn Strategy>> = vec![
        Box::new(strategies::stoch::Stoch::new().unwrap()),
        Box::new(strategies::macd_dual::MacdDual::new().unwrap()),
        Box::new(strategies::ema_50200::Ema::new().unwrap()),
        Box::new(
            strategies::bollinger_bands_reversals_mt_macd::MutiTimeFrameBollingerBands::new()
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

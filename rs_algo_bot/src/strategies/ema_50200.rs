use super::strategy::*;

use async_trait::async_trait;
use rs_algo_shared::error::Result;
use rs_algo_shared::helpers::calc::*;
use rs_algo_shared::indicators::Indicator;
use rs_algo_shared::models::stop_loss::*;
use rs_algo_shared::models::{strategy, strategy::StrategyType};
use rs_algo_shared::scanner::instrument::*;

#[derive(Clone)]
pub struct Ema<'a> {
    name: &'a str,
    strategy_type: StrategyType,
    stop_loss: StopLoss,
}

#[async_trait]
impl<'a> Strategy for Ema<'a> {
    fn new() -> Result<Self> {
        let stop_loss = std::env::var("ATR_STOP_LOSS")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let strategy_type = std::env::var("STRATEGY_TYPE").unwrap();

        Ok(Self {
            name: "EMA_50_200_LS",
            strategy_type: strategy::from_str(&strategy_type),
            stop_loss: init_stop_loss(StopLossType::Atr, stop_loss),
        })
    }

    fn name(&self) -> &str {
        self.name
    }

    fn strategy_type(&self) -> &StrategyType {
        &self.strategy_type
    }

    fn update_stop_loss(&mut self, stop_type: StopLossType, price: f64) -> &StopLoss {
        self.stop_loss = update_stop_loss_values(&self.stop_loss, stop_type, price);
        &self.stop_loss
    }

    fn stop_loss(&self) -> &StopLoss {
        &self.stop_loss
    }

    fn entry_long(
        &mut self,
        instrument: &Instrument,
        _upper_tf_instrument: &HigherTMInstrument,
    ) -> bool {
        let index = instrument.data().len() - 1;
        let prev_index = get_prev_index(index);

        let last_candle = instrument.data().last().unwrap();
        let is_closed = last_candle.is_closed();

        let current_ema_50 = instrument.indicators.ema_b.get_data_a().last().unwrap();
        let current_ema_200 = instrument.indicators.ema_c.get_data_a().last().unwrap();

        let prev_ema_200 = instrument
            .indicators
            .ema_c
            .get_data_a()
            .get(prev_index)
            .unwrap();
        let prev_ema_50 = instrument
            .indicators
            .ema_b
            .get_data_a()
            .get(prev_index)
            .unwrap();

        is_closed && current_ema_50 > current_ema_200 && prev_ema_50 <= prev_ema_200
    }

    fn exit_long(
        &mut self,
        instrument: &Instrument,
        _upper_tf_instrument: &HigherTMInstrument,
    ) -> bool {
        let index = instrument.data().len() - 1;
        let prev_index = get_prev_index(index);
        let last_candle = instrument.data().last().unwrap();
        let is_closed = last_candle.is_closed();

        let current_ema_50 = instrument.indicators.ema_b.get_data_a().last().unwrap();
        let current_ema_200 = instrument.indicators.ema_c.get_data_a().last().unwrap();

        let prev_ema_200 = instrument
            .indicators
            .ema_c
            .get_data_a()
            .get(prev_index)
            .unwrap();
        let prev_ema_50 = instrument
            .indicators
            .ema_b
            .get_data_a()
            .get(prev_index)
            .unwrap();

        is_closed && current_ema_50 < current_ema_200 && prev_ema_50 >= prev_ema_200
    }

    fn entry_short(
        &mut self,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
    ) -> bool {
        match self.strategy_type {
            StrategyType::LongShort => self.exit_long(instrument, upper_tf_instrument),
            StrategyType::LongShortMultiTF => self.exit_long(instrument, upper_tf_instrument),
            StrategyType::OnlyShort => self.exit_long(instrument, upper_tf_instrument),
            _ => false,
        }
    }

    fn exit_short(
        &mut self,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
    ) -> bool {
        match self.strategy_type {
            StrategyType::LongShort => self.entry_long(instrument, upper_tf_instrument),
            StrategyType::LongShortMultiTF => self.entry_long(instrument, upper_tf_instrument),
            StrategyType::OnlyShort => self.entry_long(instrument, upper_tf_instrument),
            _ => false,
        }
    }
}

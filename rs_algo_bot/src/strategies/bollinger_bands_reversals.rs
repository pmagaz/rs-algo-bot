use super::strategy::*;

use async_trait::async_trait;
use rs_algo_shared::error::Result;
use rs_algo_shared::helpers::calc::*;
use rs_algo_shared::indicators::Indicator;
use rs_algo_shared::models::stop_loss::*;
use rs_algo_shared::models::{strategy, strategy::StrategyType};
use rs_algo_shared::scanner::instrument::*;

#[derive(Clone)]
pub struct BollingerBandsReversals<'a> {
    name: &'a str,
    strategy_type: StrategyType,
    stop_loss: StopLoss,
}

#[async_trait]
impl<'a> Strategy for BollingerBandsReversals<'a> {
    fn new() -> Result<Self> {
        let stop_loss = std::env::var("ATR_STOP_LOSS")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let strategy_type = std::env::var("STRATEGY_TYPE").unwrap();

        Ok(Self {
            name: "Bollinger_Bands_Reversals",
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
        upper_tf_instrument: &HigherTMInstrument,
    ) -> bool {
        let index = instrument.data().len() - 1;
        let last_candle = instrument.data().last().unwrap();
        let prev_index = get_prev_index(index);
        let close_price = &instrument.data.get(index).unwrap().close;
        let prev_close = &instrument.data.get(prev_index).unwrap().close;
        let date = &instrument.data.get(index).unwrap().date;
        let is_closed = last_candle.is_closed();

        let top_band = instrument.indicators.bb.get_data_a().get(index).unwrap();
        let prev_top_band = instrument
            .indicators
            .bb
            .get_data_a()
            .get(prev_index)
            .unwrap();

        let entry_condition = is_closed && (close_price > top_band && prev_close <= prev_top_band);

        entry_condition
    }

    fn exit_long(
        &mut self,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
    ) -> bool {
        let index = instrument.data().len() - 1;
        let last_candle = instrument.data().last().unwrap();
        let prev_index = get_prev_index(index);
        let low_price = &instrument.data.get(index).unwrap().low;
        let date = &instrument.data.get(index).unwrap().date;

        let close_price = &instrument.data.get(index).unwrap().close;
        let prev_close = &instrument.data.get(prev_index).unwrap().close;
        let is_closed = last_candle.is_closed();

        let top_band = instrument.indicators.bb.get_data_a().get(index).unwrap();
        let prev_top_band = instrument
            .indicators
            .bb
            .get_data_a()
            .get(prev_index)
            .unwrap();

        let exit_condition = is_closed && close_price > top_band && prev_close <= prev_top_band;

        // if exit_condition {
        //     self.update_stop_loss(StopLossType::Trailing, *low_price);
        // }
        exit_condition
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
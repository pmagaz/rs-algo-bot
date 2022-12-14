use super::strategy::*;

use async_trait::async_trait;
use rs_algo_shared::error::Result;
use rs_algo_shared::helpers::calc::*;
use rs_algo_shared::indicators::Indicator;
use rs_algo_shared::models::stop_loss::*;
use rs_algo_shared::models::{strategy, strategy::StrategyType};
use rs_algo_shared::scanner::instrument::*;

#[derive(Clone)]
pub struct MutiTimeFrameBollingerBands<'a> {
    name: &'a str,
    strategy_type: StrategyType,
    stop_loss: StopLoss,
}

#[async_trait]
impl<'a> Strategy for MutiTimeFrameBollingerBands<'a> {
    fn new() -> Result<Self> {
        let stop_loss = std::env::var("ATR_STOP_LOSS")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let strategy_type = std::env::var("STRATEGY_TYPE").unwrap();

        Ok(Self {
            name: "Bollinger_Bands_Reversals_MT_Macd",
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
        let first_htf_entry = get_bot_upper_timeframe(
            instrument,
            upper_tf_instrument,
            |(idx, prev_idx, upper_inst)| {
                let curr_upper_candle = upper_inst.data().last().unwrap();
                log::warn!("Upper {:?} ", curr_upper_candle.date());

                let curr_upper_macd_a = upper_inst.indicators.macd.get_data_a().get(idx).unwrap();
                let curr_upper_macd_b = upper_inst.indicators.macd.get_data_b().get(idx).unwrap();

                let prev_upper_macd_a = upper_inst
                    .indicators
                    .macd
                    .get_data_a()
                    .get(prev_idx)
                    .unwrap();
                let prev_upper_macd_b = upper_inst
                    .indicators
                    .macd
                    .get_data_b()
                    .get(prev_idx)
                    .unwrap();

                curr_upper_macd_a > curr_upper_macd_b && prev_upper_macd_b >= prev_upper_macd_a
            },
        );

        let upper_macd = get_bot_upper_timeframe(
            instrument,
            upper_tf_instrument,
            |(idx, _prev_idx, upper_inst)| {
                let curr_upper_macd_a = upper_inst.indicators.macd.get_data_a().get(idx).unwrap();
                let curr_upper_macd_b = upper_inst.indicators.macd.get_data_b().get(idx).unwrap();
                curr_upper_macd_a > curr_upper_macd_b
            },
        );

        let index = instrument.data().len() - 1;
        let last_candle = instrument.data().last().unwrap();
        let prev_index = get_prev_index(index);
        let close_price = &instrument.data.get(index).unwrap().close;
        let prev_close = &instrument.data.get(prev_index).unwrap().close;
        let _date = &instrument.data.get(index).unwrap().date;
        let is_closed = last_candle.is_closed();

        log::warn!("Current {:?} ", last_candle.date());

        let top_band = instrument.indicators.bb.get_data_a().get(index).unwrap();
        let prev_top_band = instrument
            .indicators
            .bb
            .get_data_a()
            .get(prev_index)
            .unwrap();

        is_closed && first_htf_entry
            || (upper_macd && close_price > top_band && prev_close <= prev_top_band)
    }

    fn exit_long(
        &mut self,
        instrument: &Instrument,
        upper_tf_instrument: &HigherTMInstrument,
    ) -> bool {
        let _upper_macd = get_bot_upper_timeframe(
            instrument,
            upper_tf_instrument,
            |(idx, prev_idx, upper_inst)| {
                let curr_upper_macd_a = upper_inst.indicators.macd.get_data_a().get(idx).unwrap();
                let curr_upper_macd_b = upper_inst.indicators.macd.get_data_b().get(idx).unwrap();

                let _prev_upper_macd_a = upper_inst
                    .indicators
                    .macd
                    .get_data_a()
                    .get(prev_idx)
                    .unwrap();
                let _prev_upper_macd_b = upper_inst
                    .indicators
                    .macd
                    .get_data_b()
                    .get(prev_idx)
                    .unwrap();
                curr_upper_macd_a < curr_upper_macd_b // && prev_upper_macd_a >= prev_upper_macd_b
            },
        );

        let index = instrument.data().len() - 1;
        let last_candle = instrument.data().last().unwrap();
        let prev_index = get_prev_index(index);
        let _low_price = &instrument.data.get(index).unwrap().low;
        let _date = &instrument.data.get(index).unwrap().date;

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

        // if exit_condition {
        //     self.update_stop_loss(StopLossType::Trailing, *low_price);
        // }

        is_closed && close_price > top_band && prev_close <= prev_top_band
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

use super::strategy::*;

use async_trait::async_trait;
use rs_algo_shared::error::Result;
use rs_algo_shared::helpers::calc::*;
use rs_algo_shared::indicators::Indicator;
use rs_algo_shared::models::stop_loss::*;
use rs_algo_shared::models::{strategy, strategy::StrategyType};
use rs_algo_shared::scanner::instrument::*;

#[derive(Clone)]
pub struct MacdDual<'a> {
    name: &'a str,
    strategy_type: StrategyType,
    stop_loss: StopLoss,
}

#[async_trait]
impl<'a> Strategy for MacdDual<'a> {
    fn new() -> Result<Self> {
        let stop_loss = std::env::var("ATR_STOP_LOSS")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let strategy_type = std::env::var("STRATEGY_TYPE").unwrap();

        Ok(Self {
            name: "Macd_Dual",
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

    fn entry_long(&mut self, instrument: &Instrument, upper_tf_instrument: &HTFInstrument) -> bool {
        let index = instrument.data().len() - 1;

        let first_htf_entry = get_upper_timeframe_data(
            index,
            instrument,
            upper_tf_instrument,
            |(_idx, prev_idx, upper_inst)| {
                let curr_upper_macd_a = upper_inst.indicators.macd.get_data_a().last().unwrap();
                let curr_upper_macd_b = upper_inst.indicators.macd.get_data_b().last().unwrap();

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

        let upper_macd = get_upper_timeframe_data(
            index,
            instrument,
            upper_tf_instrument,
            |(_idx, _prev_idx, upper_inst)| {
                let curr_upper_macd_a = upper_inst.indicators.macd.get_data_a().last().unwrap();
                let curr_upper_macd_b = upper_inst.indicators.macd.get_data_b().last().unwrap();
                curr_upper_macd_a > curr_upper_macd_b
            },
        );

        let prev_index = get_prev_index(index);
        let last_candle = instrument.data().last().unwrap();
        let is_closed = last_candle.is_closed();

        let current_macd_a = instrument.indicators.macd.get_data_a().last().unwrap();
        let current_macd_b = instrument.indicators.macd.get_data_b().last().unwrap();

        let prev_macd_a = instrument
            .indicators
            .macd
            .get_data_a()
            .get(prev_index)
            .unwrap();
        let prev_macd_b = instrument
            .indicators
            .macd
            .get_data_a()
            .get(prev_index)
            .unwrap();

        first_htf_entry
            || (is_closed
                && upper_macd
                && current_macd_a > current_macd_b
                && prev_macd_b >= prev_macd_a)
    }

    fn exit_long(&mut self, instrument: &Instrument, upper_tf_instrument: &HTFInstrument) -> bool {
        let index = instrument.data().len() - 1;
        let prev_index = get_prev_index(index);

        let first_htf_exit = get_upper_timeframe_data(
            index,
            instrument,
            upper_tf_instrument,
            |(_idx, prev_idx, upper_inst)| {
                let curr_upper_macd_a = upper_inst.indicators.macd.get_data_a().last().unwrap();
                let curr_upper_macd_b = upper_inst.indicators.macd.get_data_b().last().unwrap();

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
                curr_upper_macd_a < curr_upper_macd_b && prev_upper_macd_a >= prev_upper_macd_b
            },
        );

        let last_candle = instrument.data().last().unwrap();
        let is_closed = last_candle.is_closed();

        let current_macd_a = instrument.indicators.macd.get_data_a().last().unwrap();
        let current_macd_b = instrument.indicators.macd.get_data_b().last().unwrap();
        let _low_price = &instrument.data.get(index).unwrap().low;
        let prev_macd_a = instrument
            .indicators
            .macd
            .get_data_a()
            .get(prev_index)
            .unwrap();
        let prev_macd_b = instrument
            .indicators
            .macd
            .get_data_a()
            .get(prev_index)
            .unwrap();

        // if exit_condition {
        //     self.update_stop_loss(StopLossType::Trailing, *low_price);
        // }

        first_htf_exit
            || (is_closed && current_macd_a < current_macd_b && prev_macd_b <= prev_macd_a)
    }

    fn entry_short(
        &mut self,
        instrument: &Instrument,
        upper_tf_instrument: &HTFInstrument,
    ) -> bool {
        match self.strategy_type {
            StrategyType::LongShort => self.exit_long(instrument, upper_tf_instrument),
            StrategyType::LongShortMultiTF => self.exit_long(instrument, upper_tf_instrument),
            StrategyType::OnlyShort => self.exit_long(instrument, upper_tf_instrument),
            _ => false,
        }
    }

    fn exit_short(&mut self, instrument: &Instrument, upper_tf_instrument: &HTFInstrument) -> bool {
        match self.strategy_type {
            StrategyType::LongShort => self.entry_long(instrument, upper_tf_instrument),
            StrategyType::LongShortMultiTF => self.entry_long(instrument, upper_tf_instrument),
            StrategyType::OnlyShort => self.entry_long(instrument, upper_tf_instrument),
            _ => false,
        }
    }
}

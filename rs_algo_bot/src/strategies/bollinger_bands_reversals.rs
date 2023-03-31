use super::strategy::*;

use rs_algo_shared::error::Result;
use rs_algo_shared::helpers::calc;
use rs_algo_shared::indicators::Indicator;
use rs_algo_shared::models::order::{OrderDirection, OrderType};
use rs_algo_shared::models::pricing::Pricing;
use rs_algo_shared::models::stop_loss::*;
use rs_algo_shared::models::strategy::StrategyType;
use rs_algo_shared::models::time_frame;
use rs_algo_shared::models::time_frame::{TimeFrame, TimeFrameType};
use rs_algo_shared::models::trade::{Position, TradeDirection, TradeIn};
use rs_algo_shared::scanner::instrument::*;

#[derive(Clone)]
pub struct BollingerBandsReversals<'a> {
    name: &'a str,
    time_frame: TimeFrameType,
    higher_time_frame: Option<TimeFrameType>,
    strategy_type: StrategyType,
    trading_direction: TradeDirection,
    order_size: f64,
    risk_reward_ratio: f64,
    profit_target: f64,
}

impl<'a> Strategy for BollingerBandsReversals<'a> {
    fn new(
        time_frame: Option<&str>,
        higher_time_frame: Option<&str>,
        strategy_type: Option<StrategyType>,
    ) -> Result<Self> {
        let risk_reward_ratio = std::env::var("RISK_REWARD_RATIO")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let profit_target = std::env::var("PIPS_PROFIT_TARGET")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let base_time_frame = &std::env::var("TIME_FRAME")
            .unwrap()
            .parse::<String>()
            .unwrap();

        let order_size = std::env::var("ORDER_SIZE").unwrap().parse::<f64>().unwrap();

        let strategy_type = match strategy_type {
            Some(stype) => stype,
            None => StrategyType::OnlyLongMTF,
        };

        let time_frame = match time_frame {
            Some(tf) => TimeFrame::new(tf),
            None => TimeFrame::new(base_time_frame),
        };

        let higher_time_frame = match higher_time_frame {
            Some(htf) => Some(TimeFrame::new(htf)),
            None => match strategy_type.is_multi_timeframe() {
                true => Some(TimeFrame::new(
                    &std::env::var("HIGHER_TIME_FRAME")
                        .unwrap()
                        .parse::<String>()
                        .unwrap(),
                )),
                false => None,
            },
        };

        let trading_direction = TradeDirection::Long;

        Ok(Self {
            name: "Bollinger_Bands_Reversals",
            time_frame,
            higher_time_frame,
            strategy_type,
            trading_direction,
            order_size,
            risk_reward_ratio,
            profit_target,
        })
    }

    fn name(&self) -> &str {
        self.name
    }

    fn strategy_type(&self) -> &StrategyType {
        &self.strategy_type
    }

    fn time_frame(&self) -> &TimeFrameType {
        &self.time_frame
    }

    fn higher_time_frame(&self) -> &Option<TimeFrameType> {
        &self.higher_time_frame
    }

    fn trading_direction(
        &mut self,
        index: usize,
        instrument: &Instrument,
        htf_instrument: &HTFInstrument,
    ) -> &TradeDirection {
        self.trading_direction = time_frame::get_htf_trading_direction(
            index,
            instrument,
            htf_instrument,
            |(idx, _prev_idx, htf_inst)| {
                // let macd_a = htf_inst.indicators.macd.get_data_a().get(idx).unwrap();
                // let macd_b = htf_inst.indicators.macd.get_data_b().get(idx).unwrap();

                let htf_ema_5 = htf_inst.indicators.ema_a.get_data_a().get(idx).unwrap();
                let htf_ema_8 = htf_inst.indicators.ema_b.get_data_a().get(idx).unwrap();

                let is_long = htf_ema_5 > htf_ema_8;
                let is_short = htf_ema_5 < htf_ema_8;

                if is_long && !is_short {
                    TradeDirection::Long
                } else if is_short && !is_long {
                    TradeDirection::Short
                } else {
                    TradeDirection::None
                }
            },
        );
        &self.trading_direction
    }

    fn entry_long(
        &mut self,
        index: usize,
        instrument: &Instrument,
        _htf_instrument: &HTFInstrument,
        pricing: &Pricing,
    ) -> Position {
        let atr_value = std::env::var("ATR_STOP_LOSS")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let prev_index = calc::get_prev_index(index);
        let data = &instrument.data();
        let candle = data.get(index).unwrap();
        let prev_candle = &data.get(prev_index).unwrap();
        let close_price = &candle.close();
        let prev_close = &prev_candle.close();
        let is_closed = candle.is_closed();

        let low_band = instrument.indicators.bb.get_data_b().get(index).unwrap();
        let prev_low_band = instrument
            .indicators
            .bb
            .get_data_b()
            .get(prev_index)
            .unwrap();

        let pips_margin = 0.1;

        let entry_condition = self.trading_direction == TradeDirection::Long
            && is_closed
            && close_price < low_band
            && (prev_close >= prev_low_band);

        let buy_price = candle.high() + calc::to_pips(pips_margin, pricing);

        match entry_condition {
            true => Position::Order(vec![
                OrderType::BuyOrderLong(OrderDirection::Up, self.order_size, buy_price),
                OrderType::StopLossLong(OrderDirection::Down, StopLossType::Atr(atr_value)),
            ]),

            false => Position::None,
        }
    }

    fn exit_long(
        &mut self,
        index: usize,
        instrument: &Instrument,
        _htf_instrument: &HTFInstrument,
        _trade_in: &TradeIn,
        _pricing: &Pricing,
    ) -> Position {
        let prev_index = calc::get_prev_index(index);
        let data = &instrument.data();
        let candle = data.get(index).unwrap();
        let prev_candle = &data.get(prev_index).unwrap();
        let close_price = &candle.close();
        let prev_high = &prev_candle.high();
        let is_closed = candle.is_closed();

        let top_band = instrument.indicators.bb.get_data_a().get(index).unwrap();
        let prev_top_band = instrument
            .indicators
            .bb
            .get_data_a()
            .get(prev_index)
            .unwrap();

        let num_ridding_bars = 3;
        let mut ridding_bars = 0;
        let previous_bars = num_ridding_bars;
        for candle in data[index - previous_bars..index].iter() {
            if candle.close() > *top_band {
                ridding_bars += 1;
            }
        }

        let exit_condition = self.trading_direction == TradeDirection::Short
            || (is_closed
                && ridding_bars < num_ridding_bars
                && close_price < top_band
                && (prev_high > prev_top_band));

        match exit_condition {
            true => Position::MarketOut(None),
            false => Position::None,
        }
    }

    fn entry_short(
        &mut self,
        index: usize,
        instrument: &Instrument,
        _htf_instrument: &HTFInstrument,
        pricing: &Pricing,
    ) -> Position {
        let atr_value = std::env::var("ATR_STOP_LOSS")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let prev_index = calc::get_prev_index(index);
        let data = &instrument.data();
        let candle = data.get(index).unwrap();
        let prev_candle = &data.get(prev_index).unwrap();
        let close_price = &candle.close();
        let prev_high = &prev_candle.high();
        let is_closed = candle.is_closed();

        let pips_margin = 0.5;
        let top_band = instrument.indicators.bb.get_data_a().get(index).unwrap();
        let prev_top_band = instrument
            .indicators
            .bb
            .get_data_a()
            .get(prev_index)
            .unwrap();

        let entry_condition = self.trading_direction == TradeDirection::Short
            && is_closed
            && close_price < top_band
            && (prev_high >= prev_top_band);

        let buy_price = candle.close() - calc::to_pips(pips_margin, pricing);

        match entry_condition {
            true => Position::Order(vec![
                OrderType::BuyOrderShort(OrderDirection::Down, self.order_size, buy_price),
                OrderType::StopLossShort(OrderDirection::Up, StopLossType::Atr(atr_value)),
            ]),

            false => Position::None,
        }
    }

    fn exit_short(
        &mut self,
        index: usize,
        instrument: &Instrument,
        _htf_instrument: &HTFInstrument,
        _trade_in: &TradeIn,
        _pricing: &Pricing,
    ) -> Position {
        let prev_index = calc::get_prev_index(index);
        let data = &instrument.data();
        let candle = data.get(index).unwrap();
        let prev_candle = &data.get(prev_index).unwrap();
        let close_price = &candle.close();
        let prev_close = &prev_candle.close();
        let is_closed = candle.is_closed();

        let low_band = instrument.indicators.bb.get_data_b().get(index).unwrap();
        let prev_low_band = instrument
            .indicators
            .bb
            .get_data_b()
            .get(prev_index)
            .unwrap();

        let num_ridding_bars = 3;
        let mut ridding_bars = 0;
        let previous_bars = num_ridding_bars;

        for candle in data[index - previous_bars..index].iter() {
            if candle.close() < *low_band {
                ridding_bars += 1;
            }
        }
        let exit_condition = self.trading_direction == TradeDirection::Long
            || (is_closed
                && ridding_bars < num_ridding_bars
                && close_price < low_band
                && (prev_close >= prev_low_band));

        match exit_condition {
            true => Position::MarketOut(None),
            false => Position::None,
        }
    }
}

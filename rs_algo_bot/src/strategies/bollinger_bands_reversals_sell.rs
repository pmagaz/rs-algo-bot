use super::strategy::*;

use rs_algo_shared::error::Result;
use rs_algo_shared::helpers::calc;
use rs_algo_shared::indicators::Indicator;
use rs_algo_shared::models::order::OrderType;
use rs_algo_shared::models::stop_loss::*;
use rs_algo_shared::models::strategy::StrategyType;
use rs_algo_shared::models::tick::InstrumentTick;
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
        name: Option<&'static str>,
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

        let name = name.unwrap_or("Bollinger_Bands_Reversals");

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
            name,
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
                let current_price = &instrument.data.last().unwrap().close();
                let htf_ema_a = htf_inst.indicators.ema_a.get_data_a().get(idx).unwrap();
                let htf_ema_b = htf_inst.indicators.ema_b.get_data_a().get(idx).unwrap();
                let htf_ema_c = htf_inst.indicators.ema_c.get_data_a().get(idx).unwrap();

                let is_long = htf_ema_a > htf_ema_b && htf_ema_c > current_price;
                let is_short = htf_ema_a < htf_ema_b && htf_ema_c < current_price;

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
        tick: &InstrumentTick,
    ) -> Position {
        let atr_stoploss = std::env::var("ATR_STOPLOSS")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let prev_index = calc::get_prev_index(index);
        let data = &instrument.data();
        let candle = data.get(index).unwrap();
        let _prev_candle = &data.get(prev_index).unwrap();
        let close_price = &candle.close();
        let _high_price = &candle.high();
        let is_closed: bool = candle.is_closed();

        let prev_candle = &data.get(prev_index).unwrap();
        let prev_close = &prev_candle.close();

        let low_band = instrument.indicators.bb.get_data_b().get(index).unwrap();
        let prev_low_band = instrument
            .indicators
            .bb
            .get_data_b()
            .get(prev_index)
            .unwrap();

        let pips_margin = std::env::var("PIPS_MARGIN")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let atr_profit_target = std::env::var("ATR_PROFIT_TARGET")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let atr_value = instrument.indicators.atr.get_data_a().get(index).unwrap();

        let entry_condition = self.trading_direction == TradeDirection::Long
            && is_closed
            && close_price < low_band
            && (prev_close > prev_low_band);

        let buy_price = candle.close() + calc::to_pips(pips_margin, tick);
        let sell_price = buy_price + (atr_profit_target * atr_value) + tick.spread();

        match entry_condition {
            true => Position::Order(vec![
                OrderType::BuyOrderLong(self.order_size, buy_price),
                OrderType::SellOrderLong(self.order_size, sell_price),
                OrderType::StopLossLong(StopLossType::Atr(atr_stoploss), buy_price),
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
        _tick: &InstrumentTick,
    ) -> Position {
        let exit_condition = self.trading_direction == TradeDirection::Short;

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
        tick: &InstrumentTick,
    ) -> Position {
        let atr_stoploss = std::env::var("ATR_STOPLOSS")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let prev_index = calc::get_prev_index(index);
        let data = &instrument.data();
        let candle = data.get(index).unwrap();
        let close_price = &candle.close();
        let _low_price = &candle.low();
        let is_closed = candle.is_closed();

        let prev_candle = &data.get(prev_index).unwrap();
        let prev_close = &prev_candle.close();

        let pips_margin = std::env::var("PIPS_MARGIN")
            .unwrap()
            .parse::<f64>()
            .unwrap();
        let top_band = instrument.indicators.bb.get_data_a().get(index).unwrap();

        let prev_top_band = instrument
            .indicators
            .bb
            .get_data_a()
            .get(prev_index)
            .unwrap();

        let atr_profit_target = std::env::var("ATR_PROFIT_TARGET")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let entry_condition = self.trading_direction == TradeDirection::Short
            && is_closed
            && close_price < top_band
            && (prev_close > prev_top_band);

        let buy_price = candle.close() - calc::to_pips(pips_margin, tick);
        let atr_value = instrument.indicators.atr.get_data_a().get(index).unwrap();
        let sell_price = buy_price - (atr_profit_target * atr_value) - tick.spread();

        match entry_condition {
            true => Position::Order(vec![
                OrderType::BuyOrderShort(self.order_size, buy_price),
                OrderType::SellOrderShort(self.order_size, sell_price),
                OrderType::StopLossShort(StopLossType::Atr(atr_stoploss), buy_price),
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
        _tick: &InstrumentTick,
    ) -> Position {
        let exit_condition = self.trading_direction == TradeDirection::Long;

        match exit_condition {
            true => Position::MarketOut(None),
            false => Position::None,
        }
    }
}

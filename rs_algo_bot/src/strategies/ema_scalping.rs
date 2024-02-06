use super::strategy::*;

use rs_algo_shared::error::Result;

use rs_algo_shared::helpers::calc;
use rs_algo_shared::indicators::Indicator;
use rs_algo_shared::models::market::MarketHours;
use rs_algo_shared::models::order::OrderType;
use rs_algo_shared::models::stop_loss::*;
use rs_algo_shared::models::strategy::StrategyType;
use rs_algo_shared::models::tick::InstrumentTick;
use rs_algo_shared::models::time_frame;
use rs_algo_shared::models::time_frame::{TimeFrame, TimeFrameType};
use rs_algo_shared::models::trade::{Position, TradeDirection, TradeIn};
use rs_algo_shared::scanner::instrument::*;

#[derive(Clone)]
pub struct EmaScalping<'a> {
    name: &'a str,
    time_frame: TimeFrameType,
    higher_time_frame: Option<TimeFrameType>,
    strategy_type: StrategyType,
    trading_direction: TradeDirection,
    order_size: f64,
    risk_reward_ratio: f64,
    profit_target: f64,
}

impl<'a> Strategy for EmaScalping<'a> {
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

        let name = name.unwrap_or("Num_Bars");

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
        _market_hours: &MarketHours,
    ) -> &TradeDirection {
        self.trading_direction = time_frame::get_htf_trading_direction(
            index,
            instrument,
            htf_instrument,
            |(idx, _prev_idx, htf_inst)| {
                let htf_ema_a = htf_inst.indicators.ema_a.get_data_a().get(idx).unwrap();
                let htf_ema_c = htf_inst.indicators.ema_c.get_data_a().get(idx).unwrap();
                let candle = htf_inst.data().last().unwrap();
                let price = &candle.close();

                let is_long = htf_ema_a > htf_ema_c && price > htf_ema_a && price > htf_ema_c;
                let is_short = htf_ema_a < htf_ema_c && price < htf_ema_a && price < htf_ema_c;

                if is_long {
                    TradeDirection::Long
                } else if is_short {
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
        let prev_index = calc::get_prev_index(index);
        let data = &instrument.data();
        let candle = data.get(index).unwrap();
        let close_price = &candle.close();
        let is_closed = candle.is_closed();
        let prev_close_price = &data.get(prev_index).unwrap().close();
        let ema_a = instrument.indicators.ema_a.get_data_a().get(index).unwrap();
        let ema_b = instrument.indicators.ema_b.get_data_a().get(index).unwrap();
        let ema_c = instrument.indicators.ema_c.get_data_a().get(index).unwrap();

        let pips_margin = std::env::var("PIPS_MARGIN")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let previous_bars = 5;
        let entry_condition = is_closed
            && ema_a > ema_b
            && ema_b > ema_c
            && close_price < ema_a
            && close_price > ema_c
            && prev_close_price > ema_a;

        let start_index = std::cmp::max(index.saturating_sub(previous_bars), 0);
        let end_index = index.saturating_sub(1);

        let highest_high = data[start_index..end_index]
            .iter()
            .map(|x| x.high())
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        let buy_price = highest_high + calc::to_pips(pips_margin, tick);
        let stop_loss = close_price - calc::to_pips(pips_margin, tick);
        let risk = (buy_price - stop_loss).abs();
        let sell_price = buy_price + (risk * self.risk_reward_ratio) + tick.spread();

        match entry_condition {
            true => Position::Order(vec![
                OrderType::BuyOrderLong(self.order_size, buy_price),
                OrderType::SellOrderLong(self.order_size, sell_price),
                OrderType::StopLossLong(StopLossType::Price(stop_loss), buy_price),
            ]),

            false => Position::None,
        }
    }

    fn exit_long(
        &mut self,
        _index: usize,
        _instrument: &Instrument,
        _htf_instrument: &HTFInstrument,
        _trade_in: &TradeIn,
        _tick: &InstrumentTick,
    ) -> Position {
        Position::None
    }

    fn entry_short(
        &mut self,
        index: usize,
        instrument: &Instrument,
        _htf_instrument: &HTFInstrument,
        tick: &InstrumentTick,
    ) -> Position {
        let prev_index = calc::get_prev_index(index);
        let data = &instrument.data();
        let candle = data.get(index).unwrap();
        let close_price = &candle.close();
        let prev_close_price = &data.get(prev_index).unwrap().close();
        let ema_a = instrument.indicators.ema_a.get_data_a().get(index).unwrap();
        let ema_b = instrument.indicators.ema_b.get_data_a().get(index).unwrap();
        let ema_c = instrument.indicators.ema_c.get_data_a().get(index).unwrap();
        let is_closed = candle.is_closed();

        let pips_margin = std::env::var("PIPS_MARGIN")
            .unwrap()
            .parse::<f64>()
            .unwrap();

        let previous_bars = 5;
        let entry_condition = is_closed
            && ema_a < ema_b
            && ema_b < ema_c
            && close_price > ema_a
            && close_price < ema_c
            && prev_close_price < ema_a;

        let start_index = std::cmp::max(index.saturating_sub(previous_bars), 0);
        let end_index = index.saturating_sub(1);

        let lowest_low = data[start_index..end_index]
            .iter()
            .map(|x| x.low())
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();

        let buy_price = lowest_low - calc::to_pips(pips_margin, tick);
        let stop_loss = close_price + calc::to_pips(pips_margin, tick);
        let risk = (stop_loss - buy_price).abs();
        let sell_price = buy_price - (risk * self.risk_reward_ratio) - tick.spread();

        match entry_condition {
            true => Position::Order(vec![
                OrderType::BuyOrderShort(self.order_size, buy_price),
                OrderType::SellOrderShort(self.order_size, sell_price),
                OrderType::StopLossShort(StopLossType::Price(stop_loss), buy_price),
            ]),

            false => Position::None,
        }
    }

    fn exit_short(
        &mut self,
        _index: usize,
        _instrument: &Instrument,
        _htf_instrument: &HTFInstrument,
        trade_in: &TradeIn,
        _tick: &InstrumentTick,
    ) -> Position {
        Position::None
    }
}

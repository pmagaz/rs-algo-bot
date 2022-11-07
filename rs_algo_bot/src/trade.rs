use crate::helpers::calc::*;
use round::round;
use rs_algo_shared::helpers::date::*;
use rs_algo_shared::indicators::Indicator;
use rs_algo_shared::models::backtest_instrument::*;
use rs_algo_shared::models::market::*;
use rs_algo_shared::models::stop_loss::*;
use rs_algo_shared::models::strategy::*;
use rs_algo_shared::models::trade::*;
use rs_algo_shared::scanner::instrument::Instrument;

pub fn resolve_trade_in(
    index: usize,
    order_size: f64,
    instrument: &Instrument,
    entry_type: TradeType,
    stop_loss: &StopLoss,
) -> TradeResult {
    if entry_type == TradeType::EntryLong || entry_type == TradeType::EntryShort {
        let nex_candle_index = index + 1;
        let next_day_candle = instrument.data.get(nex_candle_index);
        let next_day_price = match next_day_candle {
            Some(candle) => candle.open,
            None => -100.,
        };
        let current_date = next_day_candle.unwrap().date;

        let quantity = round(order_size / next_day_price, 3);

        TradeResult::TradeIn(TradeIn {
            index_in: nex_candle_index,
            price_in: next_day_price,
            quantity,
            stop_loss: create_stop_loss(&entry_type, instrument, nex_candle_index, stop_loss),
            date_in: to_dbtime(current_date),
            trade_type: entry_type,
        })
    } else {
        TradeResult::None
    }
}

pub fn resolve_trade_out(
    index: usize,
    instrument: &Instrument,
    trade_in: TradeIn,
    exit_type: TradeType,
) -> TradeResult {
    let quantity = trade_in.quantity;
    let data = &instrument.data;
    let nex_candle_index = index + 1;
    let index_in = trade_in.index_in;
    let price_in = trade_in.price_in;
    let current_candle = data.get(nex_candle_index);
    let current_price = match current_candle {
        Some(candle) => candle.open,
        None => -100.,
    };

    let date_in = instrument.data.get(index_in).unwrap().date;
    let date_out = current_candle.unwrap().date;
    let profit = calculate_profit(quantity, price_in, current_price);
    let profit_per = calculate_profit_per(price_in, current_price);
    let run_up = calculate_runup(data, price_in, index_in, nex_candle_index);
    let run_up_per = calculate_runup_per(run_up, price_in);
    let draw_down = calculate_drawdown(data, price_in, index_in, nex_candle_index);
    let draw_down_per = calculate_drawdown_per(draw_down, price_in);

    let stop_loss_activated = resolve_stop_loss(current_price, &trade_in);

    if index > trade_in.index_in
        && (exit_type == TradeType::ExitLong
            || exit_type == TradeType::ExitShort
            || stop_loss_activated)
    {
        let trade_type = match stop_loss_activated {
            true => TradeType::StopLoss,
            false => exit_type,
        };

        TradeResult::TradeOut(TradeOut {
            index_in,
            price_in,
            trade_type,
            date_in: to_dbtime(date_in),
            index_out: nex_candle_index,
            price_out: current_price,
            date_out: to_dbtime(date_out),
            profit,
            profit_per,
            run_up,
            run_up_per,
            draw_down,
            draw_down_per,
        })
    } else {
        TradeResult::None
    }
}

// pub fn resolve_trades(trade_out: TradeResult, trade_in: TradeResult) -> {

//     let out = match trade_out {
//         TradeResult::TradeOut(trade_out )
//     }

// }

pub fn init_stop_loss(stop_type: StopLossType, value: f64) -> StopLoss {
    StopLoss {
        price: 0.,
        value,
        stop_type,
        created_at: to_dbtime(Local::now()),
        updated_at: to_dbtime(Local::now()),
        valid_until: to_dbtime(Local::now() + Duration::days(1000)),
    }
}

pub fn create_stop_loss(
    entry_type: &TradeType,
    instrument: &Instrument,
    index: usize,
    stop_loss: &StopLoss,
) -> StopLoss {
    let current_price = &instrument.data.get(index).unwrap().open;
    let stop_loss_value = stop_loss.value;
    let stop_loss_price = stop_loss.price;

    let price = match stop_loss.stop_type {
        StopLossType::Atr => {
            let atr_value =
                instrument.indicators.atr.get_data_a().get(index).unwrap() * stop_loss_value;
            let price = match entry_type {
                TradeType::EntryLong => current_price - atr_value,
                TradeType::EntryShort => current_price + atr_value,
                _ => current_price - atr_value,
            };
            price
        }
        _ => stop_loss_price,
    };

    // let price = match entry_type {
    //     TradeType::EntryLong => current_price - atr_value,
    //     TradeType::EntryShort => current_price + atr_value,
    //     _ => current_price - atr_value,
    // };

    StopLoss {
        price,
        value: stop_loss_value,
        stop_type: stop_loss.stop_type.to_owned(),
        created_at: to_dbtime(Local::now()),
        updated_at: to_dbtime(Local::now()),
        valid_until: to_dbtime(Local::now() + Duration::days(1000)),
    }
}

pub fn update_stop_loss_values(
    stop_loss: &StopLoss,
    stop_type: StopLossType,
    price: f64,
) -> StopLoss {
    StopLoss {
        price,
        value: stop_loss.value,
        stop_type,
        created_at: stop_loss.created_at,
        updated_at: to_dbtime(Local::now()),
        valid_until: stop_loss.valid_until,
    }
}

pub fn resolve_stop_loss(current_price: f64, trade_in: &TradeIn) -> bool {
    let stop_loss_price = trade_in.stop_loss.price;

    match trade_in.trade_type {
        TradeType::EntryLong => current_price <= stop_loss_price,
        TradeType::EntryShort => current_price >= stop_loss_price,
        _ => current_price - current_price <= stop_loss_price,
    }
}

use std::env;

use rs_algo_shared::helpers::calc::*;
use rs_algo_shared::helpers::date::to_dbtime;
use rs_algo_shared::models::tick::InstrumentTick;
use rs_algo_shared::models::trade::*;
use rs_algo_shared::models::{mode, strategy::*};

use rs_algo_shared::scanner::candle::Candle;
use rs_algo_shared::scanner::instrument::Instrument;

pub fn calculate_trade_stats(
    trade_in: &TradeIn,
    trade_out: &TradeOut,
    data: &Vec<Candle>,
) -> TradeOut {
    let execution_mode = mode::from_str(&env::var("EXECUTION_MODE").unwrap());

    let trade_type = &trade_in.trade_type;
    let date_out = match execution_mode {
        mode::ExecutionMode::Bot => trade_out.date_out,
        _ => to_dbtime(data.last().unwrap().date()),
    };

    let price_in = trade_in.price_in;
    let price_out = trade_out.price_out;
    let quantity = trade_in.quantity;

    let profit = calculate_trade_profit(quantity, price_in, price_out, trade_type);
    let profit_per = calculate_trade_profit_per(price_in, price_out, trade_type);

    let run_up = calculate_trade_runup(data, price_in, trade_type);
    let run_up_per = calculate_trade_runup_per(run_up, price_in, trade_type);
    let draw_down = calculate_trade_drawdown(data, price_in, trade_type);
    let draw_down_per = calculate_trade_drawdown_per(draw_down, price_in, trade_type);

    TradeOut {
        id: trade_out.id,
        index_in: trade_in.index_in,
        price_in: trade_in.price_in,
        ask: trade_in.ask,
        spread_in: trade_in.spread,
        trade_type: trade_out.trade_type.clone(),
        date_in: trade_in.date_in,
        index_out: trade_out.index_out,
        price_origin: trade_out.price_origin,
        price_out: trade_out.price_out,
        bid: trade_out.bid,
        spread_out: trade_in.spread,
        date_out: date_out,
        profit,
        profit_per,
        run_up,
        run_up_per,
        draw_down,
        draw_down_per,
    }
}

pub fn calculate_stats(
    instrument: &Instrument,
    trades_in: &Vec<TradeIn>,
    trades_out: &Vec<TradeOut>,
    equity: f64,
    commission: f64,
) -> StrategyStats {
    log::info!("Calculating stats");
    let _size = 1.;
    let data = &instrument.data;
    if !trades_out.is_empty() {
        let current_candle = data.last().unwrap();
        let current_price = current_candle.close;

        let w_trades: Vec<&TradeOut> = trades_out.iter().filter(|x| x.profit >= 0.).collect();
        let l_trades: Vec<&TradeOut> = trades_out.iter().filter(|x| x.profit < 0.).collect();
        let wining_trades = w_trades.len();
        let losing_trades = l_trades.len();
        let trades = wining_trades + losing_trades;
        let won_per_trade_per = avg_per_trade(&w_trades);
        let lost_per_trade_per = avg_per_trade(&l_trades);
        let stop_losses = trades_out.iter().filter(|x| x.trade_type.is_stop()).count();
        let gross_profits = total_gross(&w_trades);
        let gross_loses = total_gross(&l_trades);
        let gross_profit = gross_profits + gross_loses;
        let commissions = total_commissions(trades, commission);
        let net_profit = gross_profit - commissions;
        let first = trades_in.first().unwrap();

        let initial_order_amount = (first.price_in * first.quantity).ceil();
        let profit_factor = total_profit_factor(gross_profits, gross_loses);
        let net_profit_per = total_profit_per(equity, net_profit, trades_in, trades_out);
        let profitable_trades = total_profitable_trades(wining_trades, trades);

        let max_drawdown = match instrument.market() {
            rs_algo_shared::models::market::Market::Forex => {
                total_drawdown(trades_out, equity) * 10.
            }
            _ => total_drawdown(trades_out, equity),
        };

        let max_runup = total_runup(trades_out, equity);
        let strategy_start_price = match instrument.data.first().map(|x| x.open) {
            Some(open) => open,
            _ => 0.,
        };

        let buy_hold =
            calculate_buy_hold(strategy_start_price, initial_order_amount, current_price);
        let annual_return = 100.;

        StrategyStats {
            trades,
            wining_trades,
            losing_trades,
            won_per_trade_per,
            lost_per_trade_per,
            stop_losses,
            gross_profit,
            commissions,
            net_profit,
            net_profit_per,
            profitable_trades,
            profit_factor,
            max_runup,
            max_drawdown,
            buy_hold,
            annual_return,
        }
    } else {
        StrategyStats::new()
    }
}

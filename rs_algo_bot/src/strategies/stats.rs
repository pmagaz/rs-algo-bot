use rs_algo_shared::helpers::calc::*;
use rs_algo_shared::helpers::date::*;

use rs_algo_shared::models::strategy::*;
use rs_algo_shared::models::trade::*;

use rs_algo_shared::scanner::instrument::Instrument;
use serde::Serialize;

// #[derive(Serialize)]
// pub struct StrategyStats {
//     pub trades: usize,
//     pub wining_trades: usize,
//     pub losing_trades: usize,
//     pub won_per_trade_per: f64,
//     pub lost_per_trade_per: f64,
//     pub stop_losses: usize,
//     pub gross_profit: f64,
//     pub commissions: f64,
//     pub net_profit: f64,
//     pub net_profit_per: f64,
//     pub profitable_trades: f64,
//     pub profit_factor: f64,
//     pub max_runup: f64,
//     pub max_drawdown: f64,
//     pub buy_hold: f64,
//     pub annual_return: f64,
// }

// impl StrategyStats {
//     pub fn new() -> StrategyStats {
//         StrategyStats {
//             trades: 0,
//             wining_trades: 0,
//             losing_trades: 0,
//             won_per_trade_per: 0.,
//             lost_per_trade_per: 0.,
//             stop_losses: 0,
//             gross_profit: 0.,
//             commissions: 0.,
//             net_profit: 0.,
//             net_profit_per: 0.,
//             profitable_trades: 0.,
//             profit_factor: 0.,
//             max_runup: 0.,
//             max_drawdown: 0.,
//             buy_hold: 0.,
//             annual_return: 0.,
//         }
//     }
// }

pub fn calculate_stats(
    instrument: &Instrument,
    trades_in: &Vec<TradeIn>,
    trades_out: &Vec<TradeOut>,
    equity: f64,
    commission: f64,
) -> StrategyStats {
    let _size = 1.;
    let data = &instrument.data;
    if !trades_out.is_empty() {
        let _date_start = trades_out[0].date_in;
        let _date_end = trades_out.last().unwrap().date_out;
        let _sessions: usize = trades_out.iter().fold(0, |mut acc, x| {
            acc += x.index_out - x.index_in;
            acc
        });
        let current_candle = data.last().unwrap();
        let current_price = current_candle.close;

        let w_trades: Vec<&TradeOut> = trades_out.iter().filter(|x| x.profit > 0.).collect();
        let l_trades: Vec<&TradeOut> = trades_out.iter().filter(|x| x.profit <= 0.).collect();
        let wining_trades = w_trades.len();
        let losing_trades = l_trades.len();
        let trades = wining_trades + losing_trades;
        let won_per_trade_per = avg_per_trade(&w_trades);
        let lost_per_trade_per = avg_per_trade(&l_trades);
        let stop_losses = trades_out
            .iter()
            .filter(|x| x.trade_type == TradeType::StopLoss)
            .count();
        let gross_profits = total_gross(&w_trades);
        let gross_loses = total_gross(&l_trades);
        let gross_profit = gross_profits + gross_loses;
        let commissions = total_commissions(trades, commission);
        let net_profit = gross_profit - commissions;
        let first = trades_in.first().unwrap();

        let initial_order_amount = (first.price_in * first.quantity).ceil();
        let profit_factor = total_profit_factor(gross_profits, gross_loses);

        let net_profit_per = total_profit_per(equity, net_profit, trades_in, trades_out);
        //let net_profit_per = total_profit_per(equity, net_profit);
        let profitable_trades = total_profitable_trades(wining_trades, trades);
        let max_drawdown = total_drawdown(trades_out, equity);

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
        log::info!(
            "[BACKTEST] Error! backtesing {}",
            instrument.symbol.to_owned()
        );
        //BackTestResult::None
        let _fake_date = to_dbtime(Local::now() - Duration::days(1000));
        StrategyStats::new()
    }
}

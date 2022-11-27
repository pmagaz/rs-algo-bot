use rs_algo_shared::helpers::calc::*;
use rs_algo_shared::helpers::date::*;

use rs_algo_shared::models::strategy::*;
use rs_algo_shared::models::trade::*;

use rs_algo_shared::scanner::candle::Candle;
use rs_algo_shared::scanner::instrument::Instrument;

pub fn calculate_trade_stats(
    trade_in: &TradeIn,
    trade_out: &TradeOut,
    data: &Vec<Candle>,
) -> TradeOut {
    let price_in = trade_out.price_in;
    let price_out = trade_out.price_out;
    let index_in = trade_out.index_in;
    //FIXME
    let quantity = 1.;
    let index = data.len() - 1;
    let profit = calculate_profit(quantity, price_in, price_out);
    let profit_per = calculate_profit_per(price_in, price_out);
    let run_up = calculate_runup(data, price_in, index_in, index);
    let run_up_per = calculate_runup_per(run_up, price_in);
    let draw_down = calculate_drawdown(data, price_in, index_in, index);
    let draw_down_per = calculate_drawdown_per(draw_down, price_in);

    TradeOut {
        id: 0,
        index_in: trade_in.index_in,
        price_in: trade_in.price_in,
        trade_type: trade_out.trade_type.clone(),
        date_in: trade_in.date_in,
        index_out: trade_out.index_out,
        price_out: trade_out.price_out,
        date_out: trade_out.date_out,
        profit,
        profit_per,
        run_up,
        run_up_per,
        draw_down,
        draw_down_per,
    }
    //let stop_loss_activated = resolve_stop_loss(price_out, &trade_in);
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
        StrategyStats::new()
    }
}

mod bot;
mod error;
mod helpers;
mod message;
mod strategies;
use bot::Bot;

use dotenv::dotenv;
use helpers::vars::*;
use rs_algo_shared::models::strategy;
use rs_algo_shared::models::time_frame::TimeFrame;
use rs_algo_shared::ws::ws_client::WebSocket;
use std::env;

#[tokio::main]
async fn main() {
    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let server_url = env::var("WS_SERVER_URL").expect("WS_SERVER_URL not found");
    let port = env::var("WS_SERVER_PORT").expect("WS_SERVER_PORT not found");
    let concection_str = env::var("WS_SERVER_STR").expect("WS_SERVER_STR not found");
    let url = [&server_url, ":", &port, "/?", &concection_str].concat();

    let ws_client = WebSocket::connect(&url).await;

    log::info!("Connected to {} !", &url);

    let symbol = env::var("SYMBOL").unwrap();
    let strategy_name = env::var("STRATEGY_NAME").unwrap();
    let time_frame = env::var("TIME_FRAME").unwrap();
    let higher_time_frame = env::var("HIGHER_TIME_FRAME").unwrap();
    let market = env::var("MARKET").unwrap();
    let strategy_type = env::var("STRATEGY_TYPE").unwrap();

    let market = get_market(market);
    let time_frame = TimeFrame::new(&time_frame);
    let higher_time_frame = TimeFrame::new(&higher_time_frame);
    let strategy_type = strategy::from_str(&strategy_type);

    Bot::new()
        .symbol(symbol)
        .market(market)
        .time_frame(time_frame)
        .higher_time_frame(higher_time_frame)
        .strategy_name(strategy_name)
        .strategy_type(strategy_type)
        .websocket(ws_client)
        .build()
        .unwrap()
        .run()
        .await;
}

mod bot;
mod error;
mod message;
use bot::Bot;

use dotenv::dotenv;
use rs_algo_shared::models::market::*;
use rs_algo_shared::models::strategy::*;
use rs_algo_shared::models::time_frame::*;
use rs_algo_shared::ws::ws_client::WebSocket;
use std::env;

#[tokio::main]
async fn main() {
    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let server_url = env::var("WS_SERVER_URL").expect("WS_SERVER_URL not found");
    let port = env::var("WS_SERVER_PORT").expect("WS_SERVER_PORT not found");
    let url = [&server_url, ":", &port].concat();

    let ws_client = WebSocket::connect(&url).await;

    log::info!("Connected to {} !", &url);

    let symbol = env::var("SYMBOL").unwrap();
    let strategy_name = env::var("STRATEGY_NAME").unwrap();
    let time_frame = env::var("TIME_FRAME").unwrap();
    let market = env::var("MARKET").unwrap();
    let strategy_type = env::var("STRATEGY_TYPE").unwrap();

    let market = match market.as_ref() {
        "Forex" => Market::Forex,
        "Crypto" => Market::Crypto,
        _ => Market::Stock,
    };

    let time_frame = match time_frame.as_ref() {
        "W" => TimeFrameType::W,
        "D" => TimeFrameType::D,
        "H4" => TimeFrameType::H4,
        "H1" => TimeFrameType::H1,
        "M30" => TimeFrameType::M30,
        "M15" => TimeFrameType::M15,
        "M5" => TimeFrameType::M5,
        _ => TimeFrameType::M1,
    };

    let strategy_type = match strategy_type.as_ref() {
        "OnlyLong" => StrategyType::OnlyLong,
        "OnlyShort" => StrategyType::OnlyShort,
        "LongShort" => StrategyType::LongShort,
        "LongShortMultiTF" => StrategyType::LongShortMultiTF,
        "OnlyLongMultiTF" => StrategyType::OnlyLongMultiTF,
        _ => StrategyType::OnlyLongMultiTF,
    };

    Bot::new()
        .symbol(symbol)
        .market(market)
        .time_frame(time_frame)
        .strategy_name(strategy_name)
        .strategy_type(strategy_type)
        .websocket(ws_client)
        .build()
        .unwrap()
        .run()
        .await;
}

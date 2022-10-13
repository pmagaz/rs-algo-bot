use actix::prelude::*;
use dotenv::dotenv;
use rs_algo_shared::models::backtest_strategy::StrategyType;
use rs_algo_shared::ws::message::{Command, CommandType, Message, Subscribe};
use rs_algo_shared::ws::ws_client::WebSocket;
use serde::{Deserialize, Serialize};
use std::env;

#[tokio::main]
async fn main() {

    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let server_url = env::var("WS_SERVER_URL").expect("WS_SERVER_URL not found");
    let port = env::var("WS_SERVER_PORT").expect("WS_SERVER_PORT not found");

    log::info!("Connecting to {} on port {} !", server_url, port);

    let url = [&server_url, ":", &port].concat();

    let mut ws_client = WebSocket::connect(&url).await;

    let get_symbol_data = Command {
        command: "get_symbol_data",
        arguments: Some(Subscribe {
            strategy: "EMA200-2",
            strategy_type: StrategyType::OnlyLong,
            symbol: "EURUSD",
            time_frame: "W",
        }),
    };

    let subscribe_command = Command {
        command: "subscribe",
        arguments: Some(Subscribe {
            strategy: "EMA200-2",
            strategy_type: StrategyType::OnlyLong,
            symbol: "EURUSD",
            time_frame: "W",
        }),
    };

    ws_client
        .send(&serde_json::to_string(&get_symbol_data).unwrap())
        .await
        .unwrap();

    loop {
        let msg = ws_client.read().await.unwrap();
        let txt_msg = match msg {
            Message::Text(txt) => {
                log::info!("MSG received {}", txt);
                txt
            }
            Message::Ping(txt) => {
                log::info!("Ping received");
     
                ws_client.pong(b"").await;
                "hola".to_string()
            }
            _ => panic!("aaaaaa"),
        };
 
    }
}

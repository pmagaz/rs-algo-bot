mod message;

use dotenv::dotenv;
use rs_algo_shared::helpers::date::{DateTime, Duration as Dur, Local, Utc};
use rs_algo_shared::models::strategy::*;
use rs_algo_shared::ws::message::*;
use rs_algo_shared::ws::ws_client::WebSocket;
use std::env;

#[tokio::main]
async fn main() {
    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let server_url = env::var("WS_SERVER_URL").expect("WS_SERVER_URL not found");
    let port = env::var("WS_SERVER_PORT").expect("WS_SERVER_PORT not found");
    let url = [&server_url, ":", &port].concat();

    log::info!("Connecting to {} !", &url);

    let mut ws_client = WebSocket::connect(&url).await;

    let get_symbol_data = Command {
        command: CommandType::GetSymbolData,
        data: Some(Data {
            strategy: "EMA200-2",
            strategy_type: StrategyType::OnlyLong,
            symbol: "ETHEREUM",
            time_frame: "W",
        }),
    };

    ws_client
        .send(&serde_json::to_string(&get_symbol_data).unwrap())
        .await
        .unwrap();

    let subscribe_command = Command {
        command: CommandType::SubscribeStream,
        data: Some(Data {
            strategy: "EMA200-2",
            strategy_type: StrategyType::OnlyLong,
            symbol: "ETHEREUM",
            time_frame: "W",
        }),
    };

    ws_client
        .send(&serde_json::to_string(&subscribe_command).unwrap())
        .await
        .unwrap();

    let mut last_msg = Local::now();
    let msg_timeout = env::var("MSG_TIMEOUT").unwrap().parse::<u64>().unwrap();

    loop {
        let msg = ws_client.read().await.unwrap();
        match msg {
            Message::Text(txt) => {
                let msg = message::parse(&txt);
                log::info!("MSG received {:?}", msg);

                let timeout = Local::now() - Dur::milliseconds(msg_timeout as i64);
                if last_msg < timeout {
                    log::error!("No data received in last {} milliseconds", msg_timeout);
                } else {
                    last_msg = Local::now();
                }
            }
            Message::Ping(_txt) => {
                log::info!("Ping received");
                ws_client.pong(b"").await;
            }
            _ => panic!("Unexpected message type!"),
        };
    }
}

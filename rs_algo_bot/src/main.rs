use actix::prelude::*;
use dotenv::dotenv;
use rs_algo_shared::models::backtest_strategy::StrategyType;
use rs_algo_shared::ws::message::{Command, CommandType, Message, Subscribe};
use rs_algo_shared::ws::ws_client::WebSocket;
use serde::{Deserialize, Serialize};
use std::env;

#[tokio::main]
async fn main() {
    // #[derive(Clone, Message, Serialize, Deserialize, Debug)]
    // #[rtype(result = "()")]
    // pub struct ChatMessage {
    //   msg: String
    // };

    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let server_url = env::var("WS_SERVER_URL").expect("WS_SERVER_URL not found");
    let port = env::var("WS_SERVER_PORT").expect("WS_SERVER_PORT not found");

    log::info!("Connecting to {} on port {} !", server_url, port);

    let mut ws_client = WebSocket::connect(&[&server_url, ":", &port].concat()).await;
    let mut delay = tokio::time::interval(std::time::Duration::from_secs(1));
    // for _ in 0..5 {
    //     delay.tick().await;

    //     log::info!("Sending to {} on port {} !", server_url, port);

    #[derive(Clone, Message)]
    #[rtype(result = "()")]
    pub struct ChatMessage(pub String);

    // #[derive(Clone, Message, Serialize, Deserialize)]
    // #[rtype(result = "usize")]
    // pub struct Command<T>{pub command: CommandType, pub Option<T>};

    // ws_client.send(&Command(
    //     "EURSUSD_W".to_string(),
    //     "EMA200".to_string(),
    // ).as_string());

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
        .send(&serde_json::to_string(&subscribe_command).unwrap())
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
                ws_client.pong(b"");
                "hola".to_string()
            }
            _ => panic!(),
        };
        tokio::spawn(async move {
            // Process each socket concurrently.
            log::debug!("{}", &txt_msg);
        });
    }
}

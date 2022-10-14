use crate::{
    session,
    session::{Session, Sessions},
};

use crate::db;
use serde_json::Value;
use std::time::Duration;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tokio::time;
use tungstenite::protocol::Message;

use rs_algo_shared::broker::xtb::*;
use rs_algo_shared::broker::*;

pub async fn send(sessions: &mut Sessions, addr: &SocketAddr, msg: Message) {
    session::find(sessions, &addr, |session| {
        session.recipient.unbounded_send(msg).unwrap();
    })
    .await;
}

pub async fn broadcast(sessions: Sessions, addr: SocketAddr, msg: Message) {
    let sessions = sessions.lock().await;
    let broadcast_recipients = sessions
        .iter()
        .filter(|(peer_addr, _)| peer_addr != &&addr)
        .map(|(_, ws_sink)| ws_sink);

    for session in broadcast_recipients {
        session.recipient.unbounded_send(msg.clone()).unwrap();
    }
}

pub async fn handle<BK>(
    sessions: &mut Sessions,
    addr: &SocketAddr,
    msg: Message,
    broker: Arc<Mutex<BK>>,
    db_client: &mongodb::Client,
) -> Option<String>
where
    BK: Broker + Send + 'static,
{
    let data = match msg {
        Message::Ping(bytes) => {
            log::info!("Ping received");
            None
        }
        Message::Pong(_) => {
            log::info!("Pong received from {addr} ");

            session::find(sessions, &addr, |session| {
                session.update_ping();
            })
            .await;
            None
        }
        Message::Text(txt) => {
            let instrument = db::instrument::find_by_symbol(db_client, "aaaa")
                .await
                .unwrap();

            let msg: Value = serde_json::from_str(&txt).expect("Can't parse to JSON");
            log::info!("[MSG] {:?}", msg);

            let data = match msg["command"].clone() {
                Value::String(com) => match com.as_ref() {
                    "get_symbol_data" => {
                        let symbol = match &msg["arguments"]["symbol"] {
                            Value::String(s) => s,
                            _ => panic!("symbol parse error"),
                        };

                        // let time_frame = match &msg["arguments"]["time_frame"] {
                        //     Value::String(s) => s,
                        //     _ => panic!("time_frame parse error"),
                        // };

                        // let strategy = match &msg["arguments"]["strategy"] {
                        //     Value::String(s) => s,
                        //     _ => panic!("strategy parse error"),
                        // };

                        // let strategy_type = match &msg["arguments"]["strategy_type"] {
                        //     Value::String(s) => s,
                        //     _ => panic!("strategy type parse error"),
                        // };

                        let res = broker
                            .lock()
                            .await
                            .get_instrument_data(symbol, 1440, 1656109158)
                            .await
                            .unwrap();

                        let data = Some(serde_json::to_string(&res).unwrap());
                        data
                    }
                    "subscribe_symbol_data" => {
                        let symbol = match &msg["arguments"]["symbol"] {
                            Value::String(s) => s,
                            _ => panic!("symbol parse error"),
                        }
                        .clone();

                        tokio::spawn({
                            let sessions = Arc::clone(&sessions);
                            let addr = addr.clone();
                            async move {
                                let mut interval = time::interval(Duration::from_millis(10000));
                                loop {
                                    interval.tick().await;
                                    let mut sessions = Arc::clone(&sessions);
                                    let res = broker
                                        .lock()
                                        .await
                                        .get_instrument_data(&symbol, 1440, 1656109158)
                                        .await
                                        .unwrap();

                                    let data = Some(serde_json::to_string(&res).unwrap()).unwrap();
                                    self::send(&mut sessions, &addr, Message::Text(data)).await;
                                }
                            }
                        });
                        Some("".to_string())
                    }
                    &_ => {
                        log::error!("unknown command received {}", com);
                        None
                    }
                },

                _ => {
                    log::error!("Wrong command format {:?}", msg);
                    None
                }
            };

            data
        }
        //  Message::Close(msg) => {
        //     println!("Disconected {:?}", msg);
        // }
        _ => {
            println!("Error {:?}", msg);
            None
        }
    };
    data
}

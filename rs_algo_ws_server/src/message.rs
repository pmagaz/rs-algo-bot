use crate::db;
use crate::{
    session,
    session::{Session, Sessions},
};
use futures_util::{
    stream::{SplitSink, SplitStream},
    Future, SinkExt, StreamExt,
};
use rs_algo_shared::broker::*;
use rs_algo_shared::helpers::date::{DateTime, Duration as Dur, Local, Utc};
use serde_json::Value;
use std::time::Duration;
use std::{net::SocketAddr, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio::time;
use tungstenite::protocol::Message;

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

                        let time_frame = 5;
                        let max_bars = 200;
                        let from = (Local::now()
                            - Dur::milliseconds(time_frame * 60000 * max_bars as i64))
                        .timestamp();
                        let res = broker
                            .lock()
                            .await
                            .get_instrument_data(symbol, 1440, 1656109158)
                            //.get_instrument_data(symbol, time_frame, from)
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
                                let mut guard = broker.lock().await;
                                guard.get_instrument_streaming(&symbol, 1, 2).await.unwrap();
                                let mut interval = time::interval(Duration::from_millis(200));
                                let mut sessions = Arc::clone(&sessions);
                                let read_stream = guard.get_stream().await;

                                loop {
                                    tokio::select! {
                                        msg = read_stream.next() => {
                                            match msg {
                                                Some(msg) => {
                                                    let msg = msg.unwrap();
                                                    if msg.is_text() || msg.is_binary() {
                                                        self::send(&mut sessions, &addr, msg).await;
                                                    } else if msg.is_close() {
                                                        println!("4444444");
                                                        break;
                                                    }
                                                }
                                                None => {
                                                     println!("5555555");
                                                    break
                                                }
                                            }
                                        }
                                        _ = interval.tick() => {
                                            //self::send(&mut sessions, &addr, Message::Ping(b"".to_vec())).await;
                                        }
                                    }
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

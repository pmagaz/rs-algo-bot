use crate::{
    session,
    session::{Session, Sessions},
};

use crate::db;

use serde_json::Value;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tungstenite::protocol::Message;

use rs_algo_shared::broker::xtb::*;
use rs_algo_shared::broker::*;

pub async fn send(sessions: &mut Sessions, addr: &SocketAddr, msg: Message) {
    session::find(sessions, &addr, |session| {
        println!("88888888 {:?}", msg);
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
    BK: Broker,
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
        Message::Text(msg) => {
            let instrument = db::instrument::find_by_symbol(db_client, "aaaa")
                .await
                .unwrap();

            let msg: Value = serde_json::from_str(&msg).expect("Can't parse to JSON");

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
                            .await.unwrap();

                        let data = Some(serde_json::to_string(&res).unwrap());

                        // session::find(sessions, &addr, |session| {
                        //     session.update_name(symbol, strategy);
                        // });

                        data
                    },
                    "subscribe_symbol_data" => {
                        let symbol = match &msg["arguments"]["symbol"] {
                            Value::String(s) => s,
                            _ => panic!("symbol parse error"),
                        };
                       
                    // tokio::spawn(async move {
                    //     let res = broker
                    //         .lock()
                    //         .await.listen(symbol, |msg| {
                    //             let mut sessions = sessions.clone();
                    //             let addr = addr.clone();
                    //             async move{
                               
                    //             println!("66666666666 {:?}", msg);
                    //             self::send(&mut sessions, &addr, Message::Text(msg.to_string())).await;
                    //             //future::success(Ok(()))
                    //             Ok(())
                    //             }
                    //         }).await;
                    //     });

                        Some("".to_string())
                    },
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

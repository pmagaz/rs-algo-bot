use crate::{
    session,
    session::{Session, Sessions},
};

use crate::db;
use crate::error::RsAlgoErrorKind;
use serde_json::Value;
use std::{collections::HashMap, env, net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tungstenite::protocol::Message;

use rs_algo_shared::broker::xtb::*;
use rs_algo_shared::broker::*;

pub struct Message2 {
    sessions: Sessions,
    broker: Xtb,
    db_client: mongodb::Client,
}

impl Message2 {
    pub async fn new(sessions: Sessions) -> Self {
        let username = env::var("DB_USERNAME").expect("DB_USERNAME not found");
        let password = env::var("DB_PASSWORD").expect("DB_PASSWORD not found");
        let db_mem_name = env::var("MONGO_BOT_DB_NAME").expect("MONGO_BOT_DB_NAME not found");
        let db_mem_uri = env::var("MONGO_BOT_DB_URI").expect("MONGO_BOT_DB_URI not found");
        Self {
            sessions: Sessions::new(Mutex::new(HashMap::new())),
            broker: Xtb::new().await,
            db_client: db::mongo::connect(&username, &password, &db_mem_name, &db_mem_uri)
                .await
                .map_err(|_e| RsAlgoErrorKind::NoDbConnection)
                .unwrap(),
        }
    }

    pub async fn handle(
        &mut self,
        //sessions: &mut Sessions,
        addr: &SocketAddr,
        msg: Message,
        // broker: Arc<Mutex<BK>>,
        //  db_client: &mongodb::Client,
    ) -> Option<String> {
        let data = match msg {
            Message::Ping(bytes) => {
                log::info!("Ping received");
                None
            }
            Message::Pong(_) => {
                log::info!("Pong received from {addr} ");

                // self.sessions
                //     .find(sessions, &addr, |session| {
                //         session.update_ping();
                //     })
                //     .await;
                None
            }
            Message::Text(msg) => {
                let instrument = db::instrument::find_by_symbol(&self.db_client, "aaaa")
                    .await
                    .unwrap();

                log::info!("[FINDONE] {:?}", instrument);

                let msg: Value = serde_json::from_str(&msg).expect("Can't parse to JSON");
                let data = match msg["command"].clone() {
                    Value::String(com) => match com.as_ref() {
                        "subscribe" => {
                            let symbol = match &msg["arguments"]["symbol"] {
                                Value::String(s) => s,
                                _ => panic!("symbol parse error"),
                            };

                            let time_frame = match &msg["arguments"]["time_frame"] {
                                Value::String(s) => s,
                                _ => panic!("time_frame parse error"),
                            };

                            let strategy = match &msg["arguments"]["strategy"] {
                                Value::String(s) => s,
                                _ => panic!("strategy parse error"),
                            };

                            let strategy_type = match &msg["arguments"]["strategy_type"] {
                                Value::String(s) => s,
                                _ => panic!("strategy type parse error"),
                            };

                            //let broker = broker.unlock().unwrap();

                            let res = self
                                .broker
                                .get_instrument_data2(symbol, 1440, 1656109158)
                                .unwrap();

                            let data = Some(serde_json::to_string(&res).unwrap());

                            // let symbol = &[symbol, "_", time_frame].concat();
                            // let strategy = &[strategy, "_", strategy_type].concat();
                            println!("3333333333333");
                            // session::find(sessions, &addr, |session| {
                            //     session.update_name(symbol, strategy);
                            // });

                            data
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
}

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
    BK: Broker,
{
    let data = match msg {
        Message::Ping(bytes) => {
            log::info!("Ping received");
            None
        }
        Message::Pong(_) => {
            log::info!("Pong received from {addr} ");

            // session::find(sessions, &addr, |session| {
            //     session.update_ping();
            // })
            // .await;
            None
        }
        Message::Text(msg) => {
            let instrument = db::instrument::find_by_symbol(db_client, "aaaa")
                .await
                .unwrap();

            log::info!("[FINDONE] {:?}", instrument);

            let msg: Value = serde_json::from_str(&msg).expect("Can't parse to JSON");
            let data = match msg["command"].clone() {
                Value::String(com) => match com.as_ref() {
                    "subscribe" => {
                        let symbol = match &msg["arguments"]["symbol"] {
                            Value::String(s) => s,
                            _ => panic!("symbol parse error"),
                        };

                        let time_frame = match &msg["arguments"]["time_frame"] {
                            Value::String(s) => s,
                            _ => panic!("time_frame parse error"),
                        };

                        let strategy = match &msg["arguments"]["strategy"] {
                            Value::String(s) => s,
                            _ => panic!("strategy parse error"),
                        };

                        let strategy_type = match &msg["arguments"]["strategy_type"] {
                            Value::String(s) => s,
                            _ => panic!("strategy type parse error"),
                        };

                        //let broker = broker.unlock().unwrap();

                        let res = broker
                            .lock()
                            .await
                            .get_instrument_data2(symbol, 1440, 1656109158)
                            .unwrap();

                        let data = Some(serde_json::to_string(&res).unwrap());

                        // let symbol = &[symbol, "_", time_frame].concat();
                        // let strategy = &[strategy, "_", strategy_type].concat();
                        println!("3333333333333");
                        // session::find(sessions, &addr, |session| {
                        //     session.update_name(symbol, strategy);
                        // });

                        data
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

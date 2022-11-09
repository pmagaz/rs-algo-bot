use crate::handlers::*;
use crate::handlers::{session::Session, session::Sessions};
use crate::helpers::uuid;

use bson::Uuid;
use rs_algo_shared::helpers::date::{Duration as Dur, Local, Utc};
use rs_algo_shared::models::strategy;
use rs_algo_shared::models::time_frame::{TimeFrame, TimeFrameType};
use rs_algo_shared::models::trade;
use rs_algo_shared::ws::message::*;
use serde_json::Value;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;

pub async fn send(session: &Session, msg: Message) {
    session.recipient.unbounded_send(msg).unwrap();
}

pub async fn find_and_send(sessions: &mut Sessions, addr: &SocketAddr, msg: Message) {
    session::find(sessions, &addr, |session| {
        session.recipient.unbounded_send(msg).unwrap();
    })
    .await;
}

pub async fn send_connected(sessions: &mut Sessions, addr: &SocketAddr, msg: Message) {
    let msg: ResponseBody<String> = ResponseBody {
        response: ResponseType::Connected,
        payload: Option::None,
    };

    let msg: String = serde_json::to_string(&msg).unwrap();

    session::find(sessions, &addr, |session| {
        session
            .recipient
            .unbounded_send(Message::Text(msg))
            .unwrap();
        ()
    })
    .await;
}

pub async fn broadcast(sessions: &mut Sessions, addr: &SocketAddr, msg: Message) {
    let sessions = sessions.lock().await;
    let broadcast_recipients = sessions
        .iter()
        //.filter(|(peer_addr, _)| peer_addr != &addr)
        .map(|(_, ws_sink)| ws_sink);

    for session in broadcast_recipients {
        session.recipient.unbounded_send(msg.clone()).unwrap();
    }
}

pub async fn handle<BK>(
    sessions: &mut Sessions,
    //session: &mut Session,
    addr: &SocketAddr,
    msg: Message,
    broker: Arc<Mutex<BK>>,
    db_client: &mongodb::Client,
) -> Option<String>
where
    BK: stream::BrokerStream + Send + 'static,
{
    let data = match msg {
        Message::Ping(_bytes) => {
            log::info!("Ping received from {addr}");
            None
        }
        Message::Pong(_) => {
            log::info!("Pong received from {addr} ");

            session::find(sessions, &addr, |session| {
                *session = session.update_ping().clone();
            })
            .await;

            None
        }
        Message::Text(msg) => {
            let query: Command<Value> =
                serde_json::from_str(&msg).expect("ERROR parsing Command JSON");

            let command = query.command;
            let symbol = match &query.data {
                Some(data) => &data["symbol"].as_str().unwrap(),
                None => "",
            };

            log::info!("Client {:?} msg received from {addr}", command);

            let data = match command {
                CommandType::GetInstrumentData => {
                    let max_bars = 200;
                    let mut time_frame: TimeFrameType = TimeFrameType::ERR;

                    let session_data = match &query.data {
                        Some(data) => {
                            let seed = [
                                data["symbol"].as_str().unwrap(),
                                data["strategy"].as_str().unwrap(),
                                data["time_frame"].as_str().unwrap(),
                                data["strategy_type"].as_str().unwrap(),
                            ];

                            time_frame = TimeFrame::new(seed[2]);

                            Some(SessionData {
                                id: uuid::generate(seed),
                                symbol: seed[0].to_string(),
                                strategy: seed[1].to_string(),
                                time_frame: time_frame.clone(),
                                strategy_type: strategy::from_str(seed[3]),
                            })
                        }
                        None => None,
                    }
                    .unwrap();

                    session::find(sessions, &addr, |session| {
                        *session = session.update_data(session_data.clone()).clone();
                    })
                    .await;

                    session::update_db_session(&session_data, db_client).await;

                    let time_frame_number = time_frame.to_number();

                    let from = (Local::now()
                        - Dur::milliseconds(time_frame_number * 60000 * max_bars as i64))
                    .timestamp();

                    let res = broker
                        .lock()
                        .await
                        .get_instrument_data(&symbol, time_frame_number as usize, from)
                        .await
                        .unwrap();

                    Some(serde_json::to_string(&res).unwrap())
                }
                CommandType::ExecuteTrade => {
                    let trade = match &query.data {
                        Some(trade) => {
                            let pricing = broker
                                .lock()
                                .await
                                .get_instrument_pricing(&symbol)
                                .await
                                .unwrap();

                            log::info!("Server {:?} pricing obtained {addr}", pricing.payload);

                            let trade_type =
                                trade::type_from_str(trade["trade_type"].as_str().unwrap());

                            match trade_type {
                                EntryLong => (),
                                _ => (),
                            };
                        }
                        None => (),
                    };
                    None
                }
                CommandType::SubscribeStream => {
                    session::find(sessions, &addr, |session| {
                        stream::listen(broker, session.clone(), addr, symbol.to_owned());
                    })
                    .await;
                    Some("".to_string())
                }
                _ => {
                    log::error!("Unknown command received {:?}", &command);
                    None
                }
            };
            data
        }
        _ => {
            log::error!("Wrong command format {:?}", msg);
            None
        }
    };
    //  Message::Close(msg) => {
    //     println!("Disconected {:?}", msg);
    // }
    // _ => {
    //     println!("Error {:?}", msg);
    //     None
    // }
    // };
    data
}

use crate::db;
use crate::handlers::*;
use crate::handlers::{session::Session, session::Sessions};

use rs_algo_shared::helpers::date;
use rs_algo_shared::helpers::date::{Duration as Dur, Local};
use rs_algo_shared::models::bot::BotData;
use rs_algo_shared::models::time_frame::*;
use rs_algo_shared::models::{trade, trade::*};
use rs_algo_shared::ws::message::*;
use serde_json::Value;
use std::env;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;

pub async fn send(session: &Session, msg: Message) {
    session.recipient.unbounded_send(msg).unwrap();
}

pub async fn find_and_send(sessions: &mut Sessions, addr: &SocketAddr, msg: Message) {
    session::find(sessions, addr, |session| {
        session.recipient.unbounded_send(msg).unwrap();
    })
    .await;
}

pub async fn send_connected(sessions: &mut Sessions, addr: &SocketAddr, _msg: Message) {
    let msg: ResponseBody<String> = ResponseBody {
        response: ResponseType::Connected,
        payload: Option::None,
    };

    let msg: String = serde_json::to_string(&msg).unwrap();

    session::find(sessions, addr, |session| {
        session
            .recipient
            .unbounded_send(Message::Text(msg))
            .unwrap();
    })
    .await;
}

pub async fn broadcast(sessions: &mut Sessions, _addr: &SocketAddr, msg: Message) {
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
    addr: &SocketAddr,
    msg: Message,
    broker: Arc<Mutex<BK>>,
    db_client: &mongodb::Client,
) -> Option<String>
where
    BK: stream::BrokerStream + Send + Sync + 'static,
{
    let data = match msg {
        Message::Ping(_bytes) => {
            log::info!("Client Ping received from {addr}");
            None
        }
        Message::Pong(_) => {
            //log::info!("Client Pong received from {addr} ");
            session::find(sessions, addr, |session| {
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
                Some(data) => data["symbol"].as_str().unwrap(),
                None => "",
            };

            // let time_frame = match &query.data {
            //     Some(data) => TimeFrame::new(data["time_frame"].as_str().unwrap()),
            //     None => TimeFrameType::ERR,
            // };

            log::info!("Client {:?} msg received from {addr}", command);

            let data = match command {
                CommandType::InitSession => {
                    let session_data = match &query.data {
                        Some(data) => {
                            let bot: BotData = serde_json::from_value(data.clone()).unwrap();
                            let uuid = bot.uuid();

                            let bot_data = match db::bot::find_by_uuid(db_client, uuid).await {
                                Some(bot) => {
                                    log::info!(
                                        "Restoring session data for {}_{} {}",
                                        data["symbol"].as_str().unwrap(),
                                        data["time_frame"].as_str().unwrap(),
                                        data["_id"].as_str().unwrap()
                                    );
                                    bot
                                }
                                None => {
                                    db::bot::insert(db_client, &bot).await.unwrap();
                                    log::info!(
                                        "Creation session data for {}_{} {}",
                                        data["symbol"].as_str().unwrap(),
                                        data["time_frame"].as_str().unwrap(),
                                        data["_id"].as_str().unwrap()
                                    );
                                    bot
                                }
                            };
                            Some(bot_data)
                        }
                        None => None,
                    }
                    .unwrap();

                    // session::find(sessions, addr, |session| {
                    //     *session = session.update_data(session_data.clone()).clone();
                    // })
                    // .await;

                    let response = ResponseBody {
                        response: ResponseType::InitSession,
                        payload: Some(session_data),
                    };
                    Some(serde_json::to_string(&response).unwrap())
                }
                CommandType::GetInstrumentPricing => {
                    let res = broker
                        .lock()
                        .await
                        .get_instrument_pricing(symbol)
                        .await
                        .unwrap();
                    log::info!("Requesting {} pricing data", symbol);

                    Some(serde_json::to_string(&res).unwrap())
                }
                CommandType::GetInstrumentData => {
                    let num_bars = env::var("NUM_BARS").unwrap().parse::<i64>().unwrap();

                    let time_frame = match &query.data {
                        Some(data) => TimeFrame::new(data["time_frame"].as_str().unwrap()),
                        None => TimeFrameType::ERR,
                    };

                    let time_frame_number = time_frame.to_number();
                    let time_frame_from = TimeFrame::get_starting_bar(num_bars, &time_frame);

                    log::info!("Requesting Instrument data from {} ", time_frame_from,);

                    let res = broker
                        .lock()
                        .await
                        .get_instrument_data(
                            symbol,
                            time_frame_number as usize,
                            time_frame_from.timestamp(),
                        )
                        .await
                        .unwrap();

                    Some(serde_json::to_string(&res).unwrap())
                }
                CommandType::ExecutePosition => {
                    let res = match &query.data {
                        Some(value) => {
                            let mut guard = broker.lock().await;
                            let symbol = value["symbol"].as_str().unwrap();
                            let position_result: PositionResult =
                                serde_json::from_value(value["data"].clone()).unwrap();

                            let txt_trade_result = match position_result {
                                PositionResult::MarketIn(
                                    TradeResult::TradeIn(trade_in),
                                    new_orders,
                                ) => {
                                    log::info!("TradeIn position received");
                                    let trade_data = TradeData {
                                        symbol: symbol.to_string(),
                                        data: trade_in,
                                    };

                                    let trade_result = guard.open_trade(trade_data).await.unwrap();

                                    serde_json::to_string(&trade_result).unwrap()
                                }
                                PositionResult::MarketOut(TradeResult::TradeOut(trade_out)) => {
                                    log::info!("TradeOut position received");
                                    let trade_data = TradeData {
                                        symbol: symbol.to_string(),
                                        data: trade_out,
                                    };
                                    let trade_result = guard.close_trade(trade_data).await.unwrap();
                                    serde_json::to_string(&trade_result).unwrap()
                                }
                                PositionResult::MarketInOrder(
                                    TradeResult::TradeIn(trade_in),
                                    order,
                                ) => {
                                    log::info!("MarketInOrder position received");
                                    let trade_data = TradeData {
                                        symbol: symbol.to_string(),
                                        data: order,
                                    };
                                    let trade_result = guard.order_in(trade_data).await.unwrap();
                                    serde_json::to_string(&trade_result).unwrap()
                                }
                                PositionResult::MarketOutOrder(
                                    TradeResult::TradeOut(trade_out),
                                    _,
                                ) => {
                                    log::info!("MarketOutOrder position received");
                                    let trade_data = TradeData {
                                        symbol: symbol.to_string(),
                                        data: trade_out,
                                    };
                                    let trade_result = guard.close_trade(trade_data).await.unwrap();
                                    serde_json::to_string(&trade_result).unwrap()
                                }
                                _ => {
                                    log::info!("NOT IMPLEMENTED");
                                    "".to_string()
                                }
                            };
                            // let symbol = &value["symbol"];
                            // let time_frame = &value["time_frame"];

                            // let txt_trade_result = match trade_type.is_entry() {
                            //     true => {
                            //         let trade_in: TradeData<TradeIn> =
                            //             serde_json::from_value(value.clone()).unwrap();

                            //         let trade_result = guard.open_trade(trade_in).await.unwrap();

                            //         serde_json::to_string(&trade_result).unwrap()
                            //     }
                            //     false => {
                            //         let trade_out: TradeData<TradeOut> =
                            //             serde_json::from_value(value.clone()).unwrap();

                            //         let trade_result = guard.close_trade(trade_out).await.unwrap();

                            //         serde_json::to_string(&trade_result).unwrap()
                            //     }
                            // };

                            Some(txt_trade_result)
                        }
                        None => None,
                    };
                    res
                }
                CommandType::UpdateBotData => {
                    match &query.data {
                        Some(data) => {
                            let instrument = [
                                data["strategy_name"].as_str().unwrap(),
                                "-",
                                data["strategy_type"].as_str().unwrap(),
                                "_",
                                data["symbol"].as_str().unwrap(),
                                "_",
                                data["time_frame"].as_str().unwrap(),
                            ]
                            .concat();
                            log::info!("Updating bot data for {:?}", instrument);

                            let bot: BotData = serde_json::from_value(data.clone()).unwrap();

                            db::bot::upsert(db_client, &bot).await.unwrap();
                        }
                        None => (),
                    }
                    None
                }
                CommandType::SubscribeStream => {
                    session::find(sessions, addr, |session| {
                        stream::listen(broker.clone(), session.clone(), symbol.to_owned());
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

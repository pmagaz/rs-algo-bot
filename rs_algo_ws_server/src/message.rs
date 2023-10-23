use crate::db;
use crate::handlers::*;
use crate::handlers::{session::Session, session::Sessions};

use rs_algo_shared::models::bot::BotData;
use rs_algo_shared::models::mode;
use rs_algo_shared::models::time_frame::*;
use rs_algo_shared::models::trade::*;
use rs_algo_shared::ws::message::*;
use serde_json::Value;
use std::env;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;

pub async fn send(
    session: &Session,
    msg: Message,
) -> Result<(), futures_channel::mpsc::TrySendError<Message>> {
    session.recipient.unbounded_send(msg)
    // match session.recipient.unbounded_send(msg) {
    //     Err(_) => {
    //         log::error!("Can't send message to {:?}", session.bot_name());
    //     }
    //     _ => (),
    // }
}

pub async fn send_reconnect(session: &Session, options: ReconnectOptions) {
    log::info!("Sending Reconnect");

    let msg: ResponseBody<ReconnectOptions> = ResponseBody {
        response: ResponseType::Reconnect,
        payload: Some(options),
    };
    let txt_msg = serde_json::to_string(&msg).unwrap();
    send(session, Message::Text(txt_msg)).await.unwrap();
}

// pub async fn broadcast(sessions: &mut Sessions, _addr: &SocketAddr, msg: Message) {
//     let sessions = sessions.lock().await;
//     let broadcast_recipients = sessions
//         .iter()
//         //.filter(|(peer_addr, _)| peer_addr != &addr)
//         .map(|(_, ws_sink)| ws_sink);

//     for session in broadcast_recipients {
//         session.recipient.unbounded_send(msg.clone()).unwrap();
//     }
// }

pub async fn handle<'a, BK>(
    sessions: &'a mut Sessions,
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

            let data = match command {
                CommandType::InitSession => {
                    let session_data = match &query.data {
                        Some(data) => {
                            let bot: BotData = serde_json::from_value(data.clone()).unwrap();
                            let uuid = bot.uuid();
                            let symbol = data["symbol"].as_str().unwrap();
                            let time_frame = data["time_frame"].as_str().unwrap();
                            let strategy_name = data["strategy_name"].as_str().unwrap();
                            let id = data["_id"].as_str().unwrap();

                            let bot_data = match db::bot::find_by_uuid(db_client, uuid).await {
                                Some(bot) => {
                                    log::info!(
                                        "Restoring session data for {}_{} {}",
                                        symbol,
                                        time_frame,
                                        id
                                    );
                                    bot
                                }
                                None => {
                                    db::bot::insert(db_client, &bot).await.unwrap();
                                    log::info!(
                                        "Creating session data for {}_{} {}",
                                        symbol,
                                        time_frame,
                                        id
                                    );
                                    bot
                                }
                            };

                            session::find(sessions, addr, |session| {
                                *session = session
                                    .update_bot_name(symbol, time_frame, strategy_name)
                                    .clone();
                            })
                            .await;

                            Some(bot_data)
                        }
                        None => None,
                    }
                    .unwrap();

                    let response = ResponseBody {
                        response: ResponseType::InitSession,
                        payload: Some(session_data),
                    };
                    Some(serde_json::to_string(&response).unwrap())
                }
                CommandType::GetMarketHours => {
                    log::info!("Requesting {} trading hours", symbol);
                    match broker.lock().await.get_market_hours(symbol).await {
                        Ok(res) => {
                            let market_hours = res.payload.clone();

                            match market_hours {
                                Some(mh) => {
                                    session::find(sessions, addr, |session| {
                                        *session = session.update_market_hours(mh).clone();
                                    })
                                    .await;
                                }
                                None => todo!(),
                            };

                            Some(serde_json::to_string(&res).unwrap())
                        }
                        Err(_) => None,
                    }
                }
                CommandType::GetInstrumentTick => {
                    let res = broker
                        .lock()
                        .await
                        .get_instrument_tick(symbol)
                        .await
                        .unwrap();

                    log::info!("Requesting {} tick data", symbol);

                    Some(serde_json::to_string(&res).unwrap())
                }
                CommandType::GetInstrumentData => {
                    let time_frame = match &query.data {
                        Some(data) => TimeFrame::new(data["time_frame"].as_str().unwrap()),
                        None => TimeFrameType::ERR,
                    };

                    let num_bars = match &query.data {
                        Some(data) => data["num_bars"].as_i64().unwrap(),
                        None => env::var("NUM_BARS").unwrap().parse::<i64>().unwrap(),
                    };

                    let execution_mode = mode::from_str(&env::var("EXECUTION_MODE").unwrap());

                    let time_frame_number = time_frame.to_number();
                    let time_frame_from =
                        TimeFrame::get_starting_bar(num_bars, &time_frame, &execution_mode);

                    log::info!(
                        "Requesting {} Instrument data since {} {:?}",
                        time_frame,
                        time_frame_from,
                        (num_bars)
                    );

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
                            let mut broker_guard = broker.lock().await;
                            let symbol = value["symbol"].as_str().unwrap();
                            let options: TradeOptions =
                                serde_json::from_value(value["options"].clone()).unwrap();

                            let position_result: PositionResult =
                                serde_json::from_value(value["data"].clone()).unwrap();

                            let txt_trade_response = match position_result {
                                PositionResult::MarketIn(
                                    TradeResult::TradeIn(trade_in),
                                    _new_orders,
                                ) => {
                                    log::info!("TradeIn position received");

                                    let trade_data = TradeData::new(symbol, trade_in, options);
                                    let trade_response =
                                        broker_guard.open_trade(trade_data).await.unwrap();

                                    serde_json::to_string(&trade_response).unwrap()
                                }
                                PositionResult::MarketOut(TradeResult::TradeOut(trade_out)) => {
                                    log::info!("TradeOut position received");

                                    let trade_data = TradeData::new(symbol, trade_out, options);
                                    let trade_response =
                                        broker_guard.close_trade(trade_data).await.unwrap();
                                    serde_json::to_string(&trade_response).unwrap()
                                }
                                PositionResult::MarketInOrder(
                                    TradeResult::TradeIn(_trade_in),
                                    order,
                                ) => {
                                    log::info!("MarketInOrder position received");

                                    let trade_data = TradeData::new(symbol, order, options);
                                    let trade_response =
                                        broker_guard.open_order(trade_data).await.unwrap();
                                    serde_json::to_string(&trade_response).unwrap()
                                }
                                PositionResult::MarketOutOrder(
                                    TradeResult::TradeOut(trade_out),
                                    order,
                                ) => {
                                    log::info!("MarketOutOrder position received");
                                    let trade_data =
                                        TradeData::new(symbol, trade_out, options.clone());

                                    let order_data = TradeData::new(symbol, order, options);
                                    let trade_response = broker_guard
                                        .close_order(trade_data, order_data)
                                        .await
                                        .unwrap();
                                    serde_json::to_string(&trade_response).unwrap()
                                }
                                _ => {
                                    todo!();
                                }
                            };

                            Some(txt_trade_response)
                        }
                        None => None,
                    };
                    res
                }
                CommandType::UpdateBotData => {
                    match &query.data {
                        Some(data) => {
                            let bot: BotData = serde_json::from_value(data.clone()).unwrap();
                            db::bot::upsert(db_client, &bot).await.unwrap();
                            session::find(sessions, addr, |session| {
                                *session = session.update_last_data().clone();
                            })
                            .await;
                        }
                        None => (),
                    }
                    None
                }
                CommandType::SubscribeStream => {
                    session::find(sessions, addr, |session| {
                        stream::listen(broker.clone(), session.clone());
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
        Message::Close(err) => {
            session::find(sessions, addr, |session| {
                log::error!("{} disconnected! {:?}", session.bot_name(), err);
            })
            .await;

            session::destroy(sessions, addr).await;
            None
        }
        _ => {
            log::error!("Wrong command format {:?}", msg);
            None
        }
    };
    data
}

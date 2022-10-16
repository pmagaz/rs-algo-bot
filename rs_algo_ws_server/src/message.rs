use crate::handlers;
use crate::{session, session::Sessions};
use rs_algo_shared::helpers::date::{Duration as Dur, Local, Utc};
use rs_algo_shared::ws::message::*;

use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;

pub async fn send(sessions: &mut Sessions, addr: &SocketAddr, msg: Message) {
    session::find(sessions, &addr, |session| {
        session.recipient.unbounded_send(msg).unwrap();
    })
    .await;
}

pub async fn send_connected(sessions: &mut Sessions, addr: &SocketAddr, msg: Message) {
    let msg: Response<String> = Response {
        command: ResponseType::Connected,
        data: Option::None,
    };

    let msg: String = serde_json::to_string(&msg).unwrap();

    session::find(sessions, &addr, |session| {
        session
            .recipient
            .unbounded_send(Message::Text(msg))
            .unwrap();
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
    BK: handlers::stream::BrokerStream + Send + 'static,
{
    let data = match msg {
        Message::Ping(_bytes) => {
            log::info!("Ping received from {addr}");
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
            let query: Command<Data> =
                serde_json::from_str(&msg).expect("ERROR parsing Command JSON");

            let command = query.command;
            let symbol = match &query.data {
                Some(arg) => arg.symbol.clone(),
                None => "",
            };

            log::info!("Msg {:?} from {addr} received", command);

            let data = match command {
                CommandType::GetSymbolData => {
                    let time_frame = 5;
                    let max_bars = 200;

                    let from = (Local::now()
                        - Dur::milliseconds(time_frame * 60000 * max_bars as i64))
                    .timestamp();

                    let res = broker
                        .lock()
                        .await
                        .get_instrument_data(&symbol, 1440, 1656109158)
                        .await
                        .unwrap();

                    Some(serde_json::to_string(&res).unwrap())
                }
                CommandType::SubscribeStream => {
                    handlers::stream::listen(broker, sessions, addr, symbol.to_owned());
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

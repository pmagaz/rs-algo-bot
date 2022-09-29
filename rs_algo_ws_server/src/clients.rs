use std::time::{Duration, Instant};
use serde_json::Value;
use crate::message::Message;
use futures_util::{
    future::{select, Either},
    StreamExt as _,
};
use tokio::{pin, sync::mpsc, time::interval};

use crate::server::{ChatServerHandle, ConnId};
use std::env;

/// Echo text & binary messages received from the client, respond to ping messages, and monitor
/// connection health to detect network issues and free up resources.
pub async fn chat_ws(
    chat_server: ChatServerHandle,
    mut session: actix_ws::Session,
    mut msg_stream: actix_ws::MessageStream,
) {
    log::info!("connected");

    let hb_interval = Duration::from_secs(env::var("HEARTBEAT_INTERVAL")
        .unwrap()
        .parse::<u64>()
        .unwrap());

        let hb_client_timeout = Duration::from_secs(env::var("HB_CLIENT_TIMEOUT")
        .unwrap()
        .parse::<u64>()
        .unwrap());

    let mut name = None;
    let mut last_heartbeat = Instant::now();
    let mut interval = interval(hb_interval);

    let (conn_tx, mut conn_rx) = mpsc::unbounded_channel();

    // unwrap: chat server is not dropped before the HTTP server
    let conn_id = chat_server.connect(conn_tx).await;

    let close_reason = loop {
        // most of the futures we process need to be stack-pinned to work with select()

        let tick = interval.tick();
        pin!(tick);

        let msg_rx = conn_rx.recv();
        pin!(msg_rx);

        // TODO: nested select is pretty gross for readability on the match
        let messages = select(msg_stream.next(), msg_rx);
        pin!(messages);

        match select(messages, tick).await {
            // commands & messages received from client
            Either::Left((Either::Left((Some(Ok(msg)), _)), _)) => {
                log::debug!("msg: {msg:?}");

                match msg {
                    Message::Ping(bytes) => {
                        last_heartbeat = Instant::now();
                        // unwrap:
                        session.pong(&bytes).await.unwrap();
                    }

                    Message::Pong(_) => {
                        last_heartbeat = Instant::now();
                    }

                    Message::Text(msg) => {

                        log::info!("MSG22222: {:?}", msg);

                        process_text_msg(&chat_server, &mut session, &msg, conn_id, &mut name)
                            .await;
                    }

                    Message::Binary(_bin) => {
                        log::warn!("unexpected binary message");
                    }

                    Message::Close(reason) => break reason,

                    _ => {
                        break None;
                    }
                }
            }

            // client WebSocket stream error
            Either::Left((Either::Left((Some(Err(err)), _)), _)) => {
                log::error!("{}", err);
                break None;
            }

            // client WebSocket stream ended
            Either::Left((Either::Left((None, _)), _)) => break None,

            // chat messages received from other room participants
            Either::Left((Either::Right((Some(chat_msg), _)), _)) => {
                session.text(chat_msg).await.unwrap();
            }

            // all connection's message senders were dropped
            Either::Left((Either::Right((None, _)), _)) => unreachable!(
                "all connection message senders were dropped; chat server may have panicked"
            ),

            // heartbeat internal tick
            Either::Right((_inst, _)) => {
                // if no heartbeat ping/pong received recently, close the connection

                if Instant::now().duration_since(last_heartbeat) > hb_client_timeout {
                    log::info!(
                        "PING NOT RECEIVED {hb_client_timeout:?}; disconnecting"
                    );
                    break None;
                }

                // send heartbeat ping
                let _ = session.ping(b"").await;
            }
        };
    };

    chat_server.disconnect(conn_id);

    // attempt to close connection gracefully
    let _ = session.close(close_reason).await;
}

async fn process_text_msg(
    chat_server: &ChatServerHandle,
    session: &mut actix_ws::Session,
    text: &str,
    conn: ConnId,
    name: &mut Option<String>,
) {
    // strip leading and trailing whitespace (spaces, newlines, etc.)
    let msg = text.trim();

     let msg: Value = serde_json::from_str(&msg).expect("Can't parse to JSON");
                log::info!("{} command received", &msg["command"]);

                match msg["command"].clone() {
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

                            let room_name = [symbol, "_", time_frame].concat();
                            let client_name = &[strategy, "_", strategy_type].concat();
                           
                            
                            chat_server.join_room(conn, room_name).await;
                            //self.symbol_subscribe(name, client_name, ctx);
                        }
                        &_ => log::error!("unknown command received {}", com),
                    },
                    _ => log::error!("wrong command format {:?}", &msg),
                };


    //chat_server.join_room(conn, "leches").await;
    //chat_server.send_message(conn, msg.to_string()).await
    // we check for /<cmd> type of messages
    // if msg.starts_with('/') {
    //     let mut cmd_args = msg.splitn(2, ' ');

    //     // unwrap: we have guaranteed non-zero string length already
    //     match cmd_args.next().unwrap() {
    //         "/list" => {
    //             log::info!("conn {conn}: listing rooms");

    //             let rooms = chat_server.list_rooms().await;

    //             for room in rooms {
    //                 session.text(room).await.unwrap();
    //             }
    //         }

    //         "/join" => match cmd_args.next() {
    //             Some(room) => {
    //                 log::info!("conn {conn}: joining room {room}");

    //                 chat_server.join_room(conn, room).await;

    //                 session.text(format!("joined {room}")).await.unwrap();
    //             }

    //             None => {
    //                 session.text("!!! room name is required").await.unwrap();
    //             }
    //         },

    //         "/name" => match cmd_args.next() {
    //             Some(new_name) => {
    //                 log::info!("conn {conn}: setting name to: {new_name}");
    //                 name.replace(new_name.to_owned());
    //             }
    //             None => {
    //                 session.text("!!! name is required").await.unwrap();
    //             }
    //         },

    //         _ => {
    //             session
    //                 .text(format!("!!! unknown command: {msg}"))
    //                 .await
    //                 .unwrap();
    //         }
    //     }
    // } else {
    //     // prefix message with our name, if assigned
    //     let msg = match name {
    //         Some(ref name) => format!("{name}: {msg}"),
    //         None => msg.to_owned(),
    //     };

    //     chat_server.send_message(conn, msg).await
    // }
}
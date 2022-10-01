//! A chat server that broadcasts a message to all sessions.
//!
//! This is a simple line-based server which accepts WebSocket sessions,
//! reads lines from those sessions, and broadcasts the lines to all other
//! connected clients.
//!
//! You can test this out by running:
//!
//!     cargo run --example server 127.0.0.1:12345
//!
//! And then in another window run:
//!
//!     cargo run --example client ws://127.0.0.1:12345/
//!
//! You can run the second command in multiple windows and then chat between the
//! two, seeing the messages from the other client as they're received. For all
//! connected clients they'll all join the same room and see everyone else's
//! messages.
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{
    collections::HashMap,
    env,
    io::Error as IoError,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::{pin, sync::mpsc, time::interval};
use serde_json::Value;
use dotenv::dotenv;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use tokio::net::{TcpListener, TcpStream};
use tungstenite::protocol::Message;

type Tx = UnboundedSender<Message>;
type Sessions = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

use rs_algo_shared::broker::xtb::*;
use rs_algo_shared::broker::*;
use rs_algo_shared::helpers::date::{DateTime, Local, Utc};
use tokio::time;

fn handle_message(msg: Message, broker: &mut Xtb) -> String {
     let data = match msg {
                    Message::Ping(bytes) => {
                        log::info!("ping received");
                            "aaaa".to_string()
                        //last_heartbeat = Instant::now();
                        // unwrap:
                        //session.pong(&bytes).await.unwrap();
                    }
                    Message::Pong(_) => {
                        log::info!("pong received");
                        "aaaa".to_string()
                        //last_heartbeat = Instant::now();
                    }

                    Message::Text(msg) => {
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

                                    let name = &[symbol, "_", time_frame].concat();
                                    let client_name = &[strategy, "_", strategy_type].concat();
                                    
                                    println!("99999999 {} {}", name, client_name);
                                    let res = broker.get_instrument_data2(symbol,1440,1656109158).unwrap();
                                    serde_json::to_string(&res).unwrap()
                                    //self.symbol_subscribe(name, client_name, ctx);
                                }
                                &_ => {
                                    log::error!("unknown command received {}", com);
                                    "aaaa".to_string()
                                }
                            },
                            _ => {
                                    log::error!("Wrong command format");
                                    "aaaa".to_string()
                                }
                        };

                        data
                    }
                    //  Message::Close(msg) => {
                    //     println!("Disconected {:?}", msg);
                    // }

                     _ => {
                         println!("Error {:?}", msg);
                        "aaaa".to_string()
                    }
            };
            data
}

fn send_message(sessions: Sessions, addr: SocketAddr, data: String){
        let peers = sessions.lock().unwrap();

    let broadcast_recipients = peers
        .iter()
        .filter(|(peer_addr, _)| peer_addr == &&addr)
        .map(|(_, ws_sink)| ws_sink);

    for recp in broadcast_recipients {
        let message = Message::Text(data.to_string());
        recp.unbounded_send(message).unwrap();
    }
}


async fn handle_connection(
    sessions: Sessions,
    raw_stream: &mut TcpStream,
    addr: SocketAddr,
) {
    println!("Incoming TCP connection from: {}", addr);

    let username = &env::var("BROKER_USERNAME").unwrap();
    let password = &env::var("BROKER_PASSWORD").unwrap();
    let mut broker = Xtb::new().await;
    broker.login(username, password).await.unwrap();
    //broker.get_symbols2();

    // let hb_interval = Duration::from_secs(env::var("HEARTBEAT_INTERVAL")
    //     .unwrap()
    //     .parse::<u64>()
    //     .unwrap());
    // let mut interval = interval(hb_interval);



    loop {
        let ws_stream = tokio_tungstenite::accept_async(&mut *raw_stream)
            .await
            .expect("Error during the websocket handshake occurred");
        println!("WebSocket connection established: {}", addr);

        // Insert the write part of this peer to the peer map.
        let (tx, rx) = unbounded();
        sessions.lock().unwrap().insert(addr, tx);

        let (outgoing, incoming) = ws_stream.split();
        let broadcast_incoming = incoming.try_for_each(|msg| {
            log::info!("MSG received from {}: {}", addr,msg.to_text().unwrap());

            let data = handle_message(msg, &mut broker);
            send_message(sessions.clone(),addr,data);
            future::ok(())
        });

        let receive_from_others = rx.map(Ok).forward(outgoing);

        pin_mut!(broadcast_incoming, receive_from_others);
        future::select(broadcast_incoming, receive_from_others).await;

        println!("{} disconnected", &addr);
        sessions.lock().unwrap().remove(&addr);
    }
}

#[tokio::main]
async fn main() -> Result<(), IoError> {
    dotenv().ok();
    let port = env::var("WS_SERVER_PORT").expect("WS_SERVER_PORT not found");
    let app_name = env::var("WS_SERVER_NAME").expect("WS_SERVER_NAME not found");

    let addr = ["0.0.0.0:", &port].concat().parse::<SocketAddr>().unwrap();
    let sessions = Sessions::new(Mutex::new(HashMap::new()));
    let socket = TcpListener::bind(&addr).await.unwrap();

    while let Ok((mut stream, addr)) = socket.accept().await {
        let sessions = sessions.clone();
        tokio::spawn(async move {
            handle_connection(sessions.clone(), &mut stream, addr).await;
        });
    }

    Ok(())
}

use dotenv::dotenv;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use std::{
    collections::HashMap,
    env,
    io::Error as IoError,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use tokio::net::{TcpListener, TcpStream};
use tungstenite::protocol::Message;

type Tx = UnboundedSender<Message>;
type Sessions = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

use rs_algo_shared::broker::xtb::*;
use rs_algo_shared::broker::*;
use tokio::time;

fn handle_message(msg: Message, broker: &mut Xtb) -> String {
    let data = match msg {
        Message::Ping(bytes) => {
            log::info!("ping received");
            "".to_string()
            //last_heartbeat = Instant::now();
            // unwrap:
            //session.pong(&bytes).await.unwrap();
        }
        Message::Pong(_) => {
            log::info!("pong received");
            "".to_string()
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
                        let res = broker
                            .get_instrument_data2(symbol, 1440, 1656109158)
                            .unwrap();
                        serde_json::to_string(&res).unwrap()
                        //self.symbol_subscribe(name, client_name, ctx);
                    }
                    &_ => {
                        log::error!("unknown command received {}", com);
                        "".to_string()
                    }
                },
                _ => {
                    log::error!("Wrong command format");
                    "".to_string()
                }
            };

            data
        }
        //  Message::Close(msg) => {
        //     println!("Disconected {:?}", msg);
        // }
        _ => {
            println!("Error {:?}", msg);
            "".to_string()
        }
    };
    data
}

fn send_message(sessions: Sessions, addr: &SocketAddr, msg: Message) {
    let recipients = sessions.lock().unwrap();
    let recipient = match recipients.get(addr) {
        Some(recp) => recp.unbounded_send(msg).unwrap(),
        None => panic!("recipient not found!"),
    };
}

fn broadcast_message(sessions: Sessions, addr: SocketAddr, msg: Message) {
    let recipients = sessions.lock().unwrap();
    let broadcast_recipients = recipients
        .iter()
        .filter(|(peer_addr, _)| peer_addr != &&addr)
        .map(|(_, ws_sink)| ws_sink);

    for recp in broadcast_recipients {
        recp.unbounded_send(msg.clone()).unwrap();
    }
}

async fn init_heart_beat(sessions: Sessions, addr: SocketAddr) {
    let sessions = sessions.clone();
    tokio::spawn(async move {
        let hb_interval = env::var("HEARTBEAT_INTERVAL")
            .unwrap()
            .parse::<u64>()
            .unwrap();

        let hb_client_timeout = env::var("HB_CLIENT_TIMEOUT")
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let mut interval = time::interval(Duration::from_millis(hb_interval));
        loop {
            interval.tick().await;
            send_message(
                sessions.clone(),
                &addr,
                Message::Ping("".as_bytes().to_vec()),
            );
        }
    });
}

async fn handle_session(sessions: Sessions, raw_stream: &mut TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);
    let username = &env::var("BROKER_USERNAME").unwrap();
    let password = &env::var("BROKER_PASSWORD").unwrap();
    let mut broker = Xtb::new().await;
    broker.login(username, password).await.unwrap();
    init_heart_beat(sessions.clone(), addr).await;

    loop {
        let ws_stream = tokio_tungstenite::accept_async(&mut *raw_stream)
            .await
            .expect("Error during the websocket handshake occurred");

        // Insert the write part of this peer to the peer map.
        let (tx, rx) = unbounded();
        sessions.lock().unwrap().insert(addr, tx);

        let (outgoing, incoming) = ws_stream.split();
        let broadcast_incoming = incoming.try_for_each(|msg| {
            println!("MSG received from {}: {}", addr, msg.to_text().unwrap());

            let data = handle_message(msg, &mut broker);
            if data != "" {
                send_message(sessions.clone(), &addr, Message::Text(data))
            };

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
            handle_session(sessions.clone(), &mut stream, addr).await;
        });
    }

    Ok(())
}

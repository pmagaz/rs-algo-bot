//! A chat server that broadcasts a message to all connections.
//!
//! This is a simple line-based server which accepts WebSocket connections,
//! reads lines from those connections, and broadcasts the lines to all other
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

use dotenv::dotenv;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use tokio::net::{TcpListener, TcpStream};
use tungstenite::protocol::Message;

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

use rs_algo_shared::broker::xtb::*;
use rs_algo_shared::broker::*;
use rs_algo_shared::helpers::date::{DateTime, Local, Utc};
use tokio::time;

async fn handle_connection(
    //data: &str,
    peer_map: PeerMap,
    raw_stream: &mut TcpStream,
    addr: SocketAddr,
) {
    println!("Incoming TCP connection from: {}", addr);

    let username = &env::var("BROKER_USERNAME").unwrap();
    let password = &env::var("BROKER_PASSWORD").unwrap();
    let mut broker = Xtb::new().await;
    broker.login(username, password).await.unwrap();
    //broker.get_symbols2();

    loop {
        let ws_stream = tokio_tungstenite::accept_async(&mut *raw_stream)
            .await
            .expect("Error during the websocket handshake occurred");
        println!("WebSocket connection established: {}", addr);

        // let res = broker.get_response2().unwrap();
        // let data = &serde_json::to_string(&res).unwrap();

        println!("3333333");

        // Insert the write part of this peer to the peer map.
        let (tx, rx) = unbounded();
        peer_map.lock().unwrap().insert(addr, tx);

        let (outgoing, incoming) = ws_stream.split();

        let broadcast_incoming = incoming.try_for_each(|msg| {
            println!(
                "Received a message from {}: {}",
                addr,
                msg.to_text().unwrap()
            );
            broker.get_symbols2();
            let res = broker.get_response2().unwrap();
            let data = &serde_json::to_string(&res).unwrap();

            let peers = peer_map.lock().unwrap();

            // We want to broadcast the message to everyone except ourselves.
            let broadcast_recipients = peers
                .iter()
                .filter(|(peer_addr, _)| peer_addr == &&addr)
                .map(|(_, ws_sink)| ws_sink);

            for recp in broadcast_recipients {
                let message = Message::Text(data.to_string());
                recp.unbounded_send(message).unwrap();
            }

            future::ok(())
        });

        let receive_from_others = rx.map(Ok).forward(outgoing);

        pin_mut!(broadcast_incoming, receive_from_others);
        future::select(broadcast_incoming, receive_from_others).await;

        println!("{} disconnected", &addr);
        peer_map.lock().unwrap().remove(&addr);
    }
}

#[tokio::main]
async fn main() -> Result<(), IoError> {
    dotenv().ok();

    let addr = "0.0.0.0:9000".parse::<SocketAddr>().unwrap();
    let state = PeerMap::new(Mutex::new(HashMap::new()));
    let socket = TcpListener::bind(&addr).await.unwrap();

    while let Ok((mut stream, addr)) = socket.accept().await {
        // let username = &env::var("BROKER_USERNAME").unwrap();
        // let password = &env::var("BROKER_PASSWORD").unwrap();
        // let mut broker = Xtb::new().await;
        // broker.login(username, password).await.unwrap();
        // broker.get_symbols2().await;
        // let state = state.clone();
        // println!("4444444444444");
        // tokio::spawn(async move {
        //     loop {
        //         let res = broker.get_response2().await.unwrap();
        //         let msg = &serde_json::to_string(&res).unwrap();
        //         handle_connection(msg, state.clone(), &mut stream, addr).await;
        //     }
        // });

        //tokio::spawn(handle_connection(state.clone(), stream, addr);
        let state = state.clone();
        tokio::spawn(async move {
            handle_connection(state.clone(), &mut stream, addr).await;
        });
    }

    Ok(())
}

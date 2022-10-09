use crate::db;
use crate::error::RsAlgoErrorKind;
use crate::heart_beat;
use crate::{message, message::*};
use crate::{
    session,
    session::{Session, Sessions},
};

use futures_channel::mpsc::unbounded;
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use std::sync::Arc;
use std::{collections::HashMap, env, net::SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tungstenite::protocol::Message;

use rs_algo_shared::broker::xtb::*;
use rs_algo_shared::broker::*;

pub async fn run(addr: String) {
    let addr = addr.parse::<SocketAddr>().unwrap();
    let socket = TcpListener::bind(&addr).await.unwrap();
    let sessions = Sessions::new(Mutex::new(HashMap::new()));

    let username = env::var("DB_USERNAME").expect("DB_USERNAME not found");
    let password = env::var("DB_PASSWORD").expect("DB_PASSWORD not found");
    let db_mem_name = env::var("MONGO_BOT_DB_NAME").expect("MONGO_BOT_DB_NAME not found");
    let db_mem_uri = env::var("MONGO_BOT_DB_URI").expect("MONGO_BOT_DB_URI not found");

    let db_client = db::mongo::connect(&username, &password, &db_mem_name, &db_mem_uri)
        .await
        .map_err(|_e| RsAlgoErrorKind::NoDbConnection)
        .unwrap();

    let message2 = Message2::<Xtb>::new(sessions, db_client).await;
    let m2 = Arc::new(Mutex::new(message2));

    while let Ok((mut stream, addr)) = socket.accept().await {
        let m2 = Arc::clone(&m2);
        tokio::spawn(async move {
            handle_session::<Xtb>(&mut stream, addr, m2).await;
        });
    }
}

async fn handle_session<BK>(
    raw_stream: &mut TcpStream,
    addr: SocketAddr,
    message2: Arc<Mutex<Message2<BK>>>,
) where
    BK: Broker,
{
    log::info!("Incoming TCP connection from: {addr}");

    let mut leches = message2.lock().await;
    leches.init_heartbeat(addr).await;

    loop {
        let ws_stream = tokio_tungstenite::accept_async(&mut *raw_stream)
            .await
            .expect("Error during the websocket handshake occurred");

        let (recipient, receiver) = unbounded();
        let new_session = Session::new(recipient);
        leches.create_session(new_session, addr).await;
        leches.check_heartbeat(addr).await;

        let (outgoing, incoming) = ws_stream.split();

        let broadcast_incoming = incoming.try_for_each(|msg| {
            let message2 = Arc::clone(&message2);
            async move {
                let mut leches = message2.lock().await;
                match leches.handle(&addr, msg).await {
                    Some(msg) => leches.send(&addr, Message::Text(msg)).await,
                    None => (),
                }
                Ok(())
            }
        });

        let receive_from_others = receiver.map(Ok).forward(outgoing);

        pin_mut!(broadcast_incoming, receive_from_others);
        future::select(broadcast_incoming, receive_from_others).await;

        //handle_disconnect(&mut sessions, &addr);

        // println!("{} disconnected", &addr);
        // sessions.lock().unwrap().remove(&addr);
    }
}

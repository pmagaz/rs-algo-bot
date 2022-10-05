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
    let sessions = Sessions::new(Mutex::new(HashMap::new()));
    let socket = TcpListener::bind(&addr).await.unwrap();

    while let Ok((mut stream, addr)) = socket.accept().await {
        let sessions = sessions.clone();
        tokio::spawn(async move {
            handle_session(sessions.clone(), &mut stream, addr).await;
        });
    }
}

async fn handle_session(mut sessions: Sessions, raw_stream: &mut TcpStream, addr: SocketAddr) {
    log::info!("Incoming TCP connection from: {addr}");

    let username = env::var("DB_USERNAME").expect("DB_USERNAME not found");
    let password = env::var("DB_PASSWORD").expect("DB_PASSWORD not found");
    let db_mem_name = env::var("MONGO_BOT_DB_NAME").expect("MONGO_BOT_DB_NAME not found");
    let db_mem_uri = env::var("MONGO_BOT_DB_URI").expect("MONGO_BOT_DB_URI not found");

    let mongo_client: mongodb::Client =
        db::mongo::connect(&username, &password, &db_mem_name, &db_mem_uri)
            .await
            .map_err(|_e| RsAlgoErrorKind::NoDbConnection)
            .unwrap();

    let username = &env::var("BROKER_USERNAME").unwrap();
    let password = &env::var("BROKER_PASSWORD").unwrap();
    let mut broker = Xtb::new().await;
    broker.login(username, password).await.unwrap();

    let bk = Arc::new(Mutex::new(broker));
    let db_c = Arc::new(mongo_client);

    heart_beat::init(&mut sessions, addr).await;

    loop {
        let ws_stream = tokio_tungstenite::accept_async(&mut *raw_stream)
            .await
            .expect("Error during the websocket handshake occurred");

        let (recipient, receiver) = unbounded();
        let new_session = Session::new(recipient);
        session::create(&mut sessions, &addr, new_session).await;
        heart_beat::check(&sessions, addr).await;

        let (outgoing, incoming) = ws_stream.split();

        let broadcast_incoming = incoming.try_for_each(|msg| {
            let broker = Arc::clone(&bk);
            let db_client = Arc::clone(&db_c);
            let mut sessions = sessions.clone();

            async move {
                match message::handle(&mut sessions, &addr, msg, broker, &db_client).await {
                    Some(msg) => message::send(&mut sessions, &addr, Message::Text(msg)).await,
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

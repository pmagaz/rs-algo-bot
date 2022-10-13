use crate::db;
use crate::error::RsAlgoErrorKind;
use crate::heart_beat;
use crate::{message, message::*};
use crate::{
    session,
    session::{Session, Sessions},
};

use futures_channel::mpsc::unbounded;
use futures_util::{future, pin_mut, stream::TryStreamExt, SinkExt, StreamExt};
use std::sync::Arc;
use std::{collections::HashMap, env, net::SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tungstenite::protocol::Message;
use rs_algo_shared::broker::xtb::*;
use rs_algo_shared::broker::*;
use rs_algo_shared::helpers::date::{DateTime, Duration as Dur, Local, Utc};
use std::time::Duration;
use tokio::time;

pub async fn leches(session: Session, session_id: String, broker: Arc<Mutex<Xtb>>) {
    tokio::spawn({
        //let broker = Arc::clone(&broker);
        let mut broker = Xtb::new().await;
    //broker.login(username, password).await.unwrap();
        let mut session = session.clone();
        async move {
                    let mut session = session.clone();
                broker
                    .listen("BITCOIN", session_id, |msg| {
                        let mut session = session.clone();
                        async move {
                            println!("2222222222{}", msg);
                            session.recipient.unbounded_send(msg).unwrap();
                            //message::send(&mut sessions, &addr, Message::Text(msg.to_string()))
                            Ok(())
                        }
                    })
                    .await;

            //let mut broker = broker.lock().await;
            //     println!("4444444444{:?}", session_id);
            // broker
            //     .get_instrument_streaming(session_id, "BITCOIN", 1000, 2)
            //     .await
            //     .unwrap();
                
            // loop {
            //     let msg = broker.stream.read().await.unwrap();
            //     println!("4444444444{:?}", &msg);
            //      session.recipient.unbounded_send(msg).unwrap();
            // }
         }
    });
}

pub async fn run(addr: String) {
    let addr = addr.parse::<SocketAddr>().unwrap();
    let sessions = Sessions::new(Mutex::new(HashMap::new()));
    let socket = TcpListener::bind(&addr).await.unwrap();

    let username = env::var("DB_USERNAME").expect("DB_USERNAME not found");
    let password = env::var("DB_PASSWORD").expect("DB_PASSWORD not found");
    let db_mem_name = env::var("MONGO_BOT_DB_NAME").expect("MONGO_BOT_DB_NAME not found");
    let db_mem_uri = env::var("MONGO_BOT_DB_URI").expect("MONGO_BOT_DB_URI not found");

    let db_client = db::mongo::connect(&username, &password, &db_mem_name, &db_mem_uri)
        .await
        .map_err(|_e| RsAlgoErrorKind::NoDbConnection)
        .unwrap();

    let username = &env::var("BROKER_USERNAME").unwrap();
    let password = &env::var("BROKER_PASSWORD").unwrap();
    let mut broker = Xtb::new().await;
    broker.login(username, password).await.unwrap();
    let broker = Arc::new(Mutex::new(broker));
    let db = Arc::new(db_client);

    while let Ok((mut stream, addr)) = socket.accept().await {
        let sessions = sessions.clone();
        let broker = Arc::clone(&broker);
        let db = Arc::clone(&db);
        tokio::spawn(async move {
            handle_session(sessions.clone(), &mut stream, addr, broker, db).await;
        });
    }
}

async fn handle_session(
    mut sessions: Sessions,
    raw_stream: &mut TcpStream,
    addr: SocketAddr,
    broker: Arc<Mutex<Xtb>>,
    db_client: Arc<mongodb::Client>,
) {
    log::info!("Incoming TCP connection from: {addr}");
    heart_beat::init(&mut sessions, addr).await;

    //let sessions2 = sessions.clone();
    let mut session_id = "".to_string();
    //let guard = broker.lock().await;
    //session_id = guard.streamSessionId.clone();

    loop {
        let ws_stream = tokio_tungstenite::accept_async(&mut *raw_stream)
            .await
            .expect("Error during the websocket handshake occurred");

        let (recipient, receiver) = unbounded();
        let new_session = Session::new(recipient);
        leches(new_session.clone(), session_id.clone(), broker.clone()).await;
        session::create(&mut sessions, &addr, new_session).await;
        heart_beat::check(&sessions, addr).await;
        
        //message::send(&mut sessions, &addr, Message::Text("conected".to_owned())).await;

        let (outgoing, incoming) = ws_stream.split();

        let broadcast_incoming = incoming.try_for_each(|msg| {
            let broker = Arc::clone(&broker);
            let db_client = Arc::clone(&db_client);
            let mut sessions = Arc::clone(&sessions);
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

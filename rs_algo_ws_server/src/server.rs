use crate::db;
use crate::error::RsAlgoErrorKind;
use crate::handlers::*;
use crate::heart_beat;
use crate::message;

use crate::handlers::session::Sessions;
use rs_algo_shared::broker::xtb_stream::*;

use futures_channel::mpsc::unbounded;
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use std::sync::Arc;
use std::{collections::HashMap, env, net::SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_tungstenite::accept_async;
use tungstenite::protocol::Message;

pub async fn run(addr: String) {
    let addr = addr.parse::<SocketAddr>().unwrap();
    let mut sessions = Sessions::new(Mutex::new(HashMap::new()));
    let socket = TcpListener::bind(&addr).await.unwrap();

    let username = env::var("DB_USERNAME").expect("DB_USERNAME not found");
    let password = env::var("DB_PASSWORD").expect("DB_PASSWORD not found");
    let db_mem_name = env::var("MONGO_BOT_DB_NAME").expect("MONGO_BOT_DB_NAME not found");
    let db_mem_uri = env::var("MONGO_BOT_DB_URI").expect("MONGO_BOT_DB_URI not found");

    let mongo_client = db::mongo::connect(&username, &password, &db_mem_name, &db_mem_uri)
        .await
        .map_err(|_e| RsAlgoErrorKind::NoDbConnection)
        .unwrap();

    // heart_beat::init(&mut sessions).await;

    let db_client = Arc::new(mongo_client);

    while let Ok((mut stream, addr)) = socket.accept().await {
        let sessions = sessions.clone();
        let db_client = Arc::clone(&db_client);

        tokio::spawn(async move {
            handle_connection(sessions, &mut stream, addr, db_client).await;
        });
    }
}

async fn handle_connection(
    mut sessions: Sessions,
    raw_stream: &mut TcpStream,
    addr: SocketAddr,
    db_client: Arc<mongodb::Client>,
) {
    loop {
        let (recipient, receiver) = unbounded();

        match accept_async(&mut *raw_stream).await {
            Ok(msg) => {
                log::info!("New connection from: {addr}");

                let username = &env::var("BROKER_USERNAME").unwrap();
                let password = &env::var("BROKER_PASSWORD").unwrap();
                let mut broker = Xtb::new().await;
                broker.login(username, password).await.unwrap();

                let broker = Arc::new(Mutex::new(broker));
                let new_session = session::create(&mut sessions, &addr, recipient).await;
                let (outgoing, incoming) = msg.split();

                let broadcast_incoming = incoming.try_for_each(|msg| {
                    let broker = Arc::clone(&broker);
                    let db_client = Arc::clone(&db_client);
                    let mut sessions = Arc::clone(&sessions);
                    let new_session = new_session.clone();
                    async move {
                        match message::handle(&mut sessions, &addr, msg, broker, &db_client).await {
                            Some(msg) => message::send(&new_session, Message::Text(msg))
                                .await
                                .unwrap(),
                            None => (),
                        }
                        Ok(())
                    }
                });

                let receive_from_others = receiver.map(Ok).forward(outgoing);
                pin_mut!(broadcast_incoming, receive_from_others);
                future::select(broadcast_incoming, receive_from_others).await;
            }
            Err(err) => {
                log::error!("Client connection error: {:?}", err);

                session::find(&mut sessions, &addr, |session| {
                    log::error!("Communication with {} {} lost!", session.bot_name(), addr);
                })
                .await;

                session::destroy(&mut sessions, &addr).await;

                break;
            }
        };
    }
}

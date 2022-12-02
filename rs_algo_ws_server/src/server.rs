use crate::db;
use crate::error::RsAlgoErrorKind;
use crate::handlers::session::Sessions;
use crate::handlers::*;
use crate::heart_beat;
use crate::message;
use rs_algo_shared::broker::xtb_stream::*;

use futures_channel::mpsc::unbounded;
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use std::sync::Arc;
use std::{collections::HashMap, env, net::SocketAddr};
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
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

    heart_beat::init(&mut sessions, addr).await;

    let db_client = Arc::new(mongo_client);

    while let Ok((mut stream, addr)) = socket.accept().await {
        let (is_bot_connection, ws_key) = is_bot_connection(&mut stream).await;
        let is_bot_connection = true;
        println!("111111 {} {}", is_bot_connection, ws_key);

        if is_bot_connection {
            let sessions = sessions.clone();
            let db_client = Arc::clone(&db_client);
            tokio::spawn(async move {
                handle_connection(sessions, &mut stream, addr, db_client).await;
            });
        } else {
            tokio::spawn(async move {
                handle_health(&mut stream, &ws_key).await;
            });
        }
    }
}

async fn handle_connection(
    mut sessions: Sessions,
    raw_stream: &mut TcpStream,
    addr: SocketAddr,
    db_client: Arc<mongodb::Client>,
) {
    log::info!("Incoming TCP connection from: {addr}");

    let username = &env::var("BROKER_USERNAME").unwrap();
    let password = &env::var("BROKER_PASSWORD").unwrap();
    let mut broker = Xtb::new().await;
    broker.login(username, password).await.unwrap();

    let broker = Arc::new(Mutex::new(broker));

    loop {
        let ws_stream = accept_async(&mut *raw_stream)
            .await
            .expect("Error during the websocket handshake occurred");

        let (recipient, receiver) = unbounded();
        let new_session = session::create(&mut sessions, &addr, recipient).await;

        let (outgoing, incoming) = ws_stream.split();
        let broker = Arc::clone(&broker);

        let broadcast_incoming = incoming.try_for_each(|msg| {
            let broker = Arc::clone(&broker);
            let db_client = Arc::clone(&db_client);
            let mut sessions = Arc::clone(&sessions);
            let new_session = new_session.clone();
            async move {
                match message::handle(&mut sessions, &addr, msg, broker, &db_client).await {
                    Some(msg) => message::send(&new_session, Message::Text(msg)).await,
                    None => (),
                }
                Ok(())
            }
        });

        let receive_from_others = receiver.map(Ok).forward(outgoing);
        pin_mut!(broadcast_incoming, receive_from_others);
        future::select(broadcast_incoming, receive_from_others).await;

        session::destroy(&mut sessions, &addr).await;
    }
}

async fn handle_health(writer: &mut TcpStream, ws_key: &str) {
    // let writer = &mut stream;
    println!("2222222");
    let response = [
        "HTTP/1.1 101 Switching Protocols\r\n",
        "Upgrade: websocket\r\n",
        "Connection: Upgrade\r\n",
        "Sec-WebSocket-Accept: ",
        ws_key,
        "\r\n\r\n",
    ]
    .concat();
    writer.write_all(response.as_bytes()).await.unwrap();
}

async fn is_bot_connection(mut stream: &mut TcpStream) -> (bool, String) {
    let mut result = false;
    let mut ws_key: String = "".to_owned();
    let reader = BufReader::new(&mut stream);
    let mut leches = reader.lines();
    for line in leches.next_line().await.unwrap() {
        match line.contains("ws_bots") {
            true => result = true,
            false => result = false,
        };

        match line.contains("Sec-WebSocket-Key") {
            true => {
                let arr: Vec<String> = line.split(": ").map(|s| s.to_string()).collect();
                ws_key = arr.last().unwrap().to_owned();
            }
            false => ws_key = "".to_string(),
        };

        println!("6666666666 {:?}", line);
    }
    // if ws_key != "".to_owned() {
    //     break;
    // }

    println!("7777777777");
    (result, ws_key)
}

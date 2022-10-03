use crate::heart_beat::*;
use crate::message::*;
use crate::session::*;

use tungstenite::protocol::Message;
use futures_channel::mpsc::unbounded;
use tokio::net::{TcpListener, TcpStream};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use std::{collections::HashMap, env, net::SocketAddr, sync::Mutex};

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

    let username = &env::var("BROKER_USERNAME").unwrap();
    let password = &env::var("BROKER_PASSWORD").unwrap();
    
    let mut broker = Xtb::new().await;
    broker.login(username, password).await.unwrap();
    init_heart_beat(&mut sessions, addr).await;

    loop {
        let ws_stream = tokio_tungstenite::accept_async(&mut *raw_stream)
            .await
            .expect("Error during the websocket handshake occurred");

        let (recipient, receiver) = unbounded();
        let new_session = Session::new(recipient);
        create_session(&mut sessions, &addr, new_session);
        check_heart_beat(&sessions, addr).await;

        let (outgoing, incoming) = ws_stream.split();
        let broadcast_incoming = incoming.try_for_each(|msg| {
            println!("received from {addr}");
            
            match handle_message(&mut sessions, &addr, msg, &mut broker) {
                Some(msg) => send_message(&mut sessions, &addr, Message::Text(msg)),
                None => (),
            }

            future::ok(())
        });

        let receive_from_others = receiver.map(Ok).forward(outgoing);

        pin_mut!(broadcast_incoming, receive_from_others);
        future::select(broadcast_incoming, receive_from_others).await;

        //handle_disconnect(&mut sessions, &addr);

        // println!("{} disconnected", &addr);
        // sessions.lock().unwrap().remove(&addr);
    }
}


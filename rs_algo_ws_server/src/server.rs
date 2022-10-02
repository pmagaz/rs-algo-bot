use crate::message;
use crate::hb;

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use std::{collections::HashMap,env,net::SocketAddr,sync::{Arc, Mutex}};

use tokio::net::{TcpListener, TcpStream};
use tungstenite::protocol::Message;

use rs_algo_shared::broker::xtb::*;
use rs_algo_shared::broker::*;

pub struct Session {
    pub sender: UnboundedSender<Message>,
    pub name: String
}

pub type Sessions = Arc<Mutex<HashMap<SocketAddr, Session>>>;

 pub async fn run(addr: String ) {

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
    //hb::init_heart_beat(sessions.clone(), addr).await;

    loop {
        let ws_stream = tokio_tungstenite::accept_async(&mut *raw_stream)
            .await
            .expect("Error during the websocket handshake occurred");

        let (sender, receiver) = unbounded();
        let new_session = Session{
            sender,
            name: "".to_string()

        };

        sessions.lock().unwrap().insert(addr, new_session);

        let (outgoing, incoming) = ws_stream.split();
        let broadcast_incoming = incoming.try_for_each(|msg| {
            println!("MSG received from {}: {}", addr, msg.to_text().unwrap());

            match message::handle_message(&mut sessions, msg, &mut broker) {
                Some(msg) => message::send_message(&mut sessions, &addr, Message::Text(msg)),
                None => log::error!("Wrong command format"),
            }

            future::ok(())
        });

        let receive_from_others = receiver.map(Ok).forward(outgoing);

        pin_mut!(broadcast_incoming, receive_from_others);
        future::select(broadcast_incoming, receive_from_others).await;

        println!("{} disconnected", &addr);
        sessions.lock().unwrap().remove(&addr);
    }
}


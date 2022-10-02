use crate::message;
use crate::session::*;

use std::env;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time;
use tungstenite::protocol::Message;

pub async fn init_heart_beat(sessions: &mut Sessions, addr: SocketAddr) {
    let mut sessions = sessions.clone();
    tokio::spawn(async move {
        let hb_interval = env::var("HEARTBEAT_INTERVAL")
            .unwrap()
            .parse::<u64>()
            .unwrap();

        let mut interval = time::interval(Duration::from_millis(hb_interval));
        loop {
            interval.tick().await;
            message::send_message(&mut sessions, &addr, Message::Ping("".as_bytes().to_vec()));
        }
    });
}

pub async fn check_heart_beat(sessions: &Sessions, addr: SocketAddr) {
    let mut sessions = sessions.clone();
    tokio::spawn(async move {
        let hb_client_timeout = env::var("HB_CLIENT_TIMEOUT")
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let mut interval = time::interval(Duration::from_millis(hb_client_timeout));

        loop {
            interval.tick().await;
            log::info!("Checking HB for {addr}");
            find_session(&mut sessions, &addr, |session| {
                //CONTINUE HERE
            });
        }
    });
}

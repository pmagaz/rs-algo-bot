use crate::server::*;
use crate::message;

use tungstenite::protocol::Message;
use std::net::SocketAddr;
use tokio::time;
use std::time::Duration;
use std::env;

pub async fn init_heart_beat(sessions: &mut Sessions, addr: SocketAddr) {
    let mut sessions = sessions.clone();
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
            message::send_message(
                &mut sessions,
                &addr,
                Message::Ping("".as_bytes().to_vec()),
            );
        }
    });
}


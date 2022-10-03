use crate::message;
use crate::session::*;
use rs_algo_shared::helpers::date::{Local, DateTime, Utc, Duration as Dur};

use std::env;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time;
use tungstenite::protocol::Message;

pub async fn init_heart_beat(sessions: &mut Sessions, addr: SocketAddr) {
    let mut sessions = sessions.clone();
    let hb_interval = env::var("HEARTBEAT_INTERVAL")
        .unwrap()
        .parse::<u64>()
        .unwrap();

    tokio::spawn(async move {
       
        let mut interval = time::interval(Duration::from_millis(hb_interval));
        loop {
            interval.tick().await;
            message::send_message(&mut sessions, &addr, Message::Ping("".as_bytes().to_vec()));
        }
    });
}

pub async fn check_heart_beat(sessions: &Sessions, addr: SocketAddr) {
    let mut sessions = sessions.clone();

    let hb_client_timeout = env::var("HB_CLIENT_TIMEOUT")
    .unwrap()
    .parse::<u64>()
    .unwrap();

    tokio::spawn(async move {

        let mut interval = time::interval(Duration::from_millis(hb_client_timeout));

        loop {
            let hb_timeout: DateTime<Local> =  Local::now() - Dur::microseconds(hb_client_timeout as i64);
            interval.tick().await;
            
            log::info!("Checking HB for {addr}");
            find_session(&mut sessions, &addr, |session| {
                let last_ping = session.last_ping;
                if last_ping < hb_timeout { 
                    session.update_client_status(SessionStatus::Down);
                    log::error!("Ping not received from {addr}");
                }
            });
        }
    });
}

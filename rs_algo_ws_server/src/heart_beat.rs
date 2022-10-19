use crate::handlers;
use crate::handlers::session::{Session, SessionStatus, Sessions};
use crate::message;

use rs_algo_shared::helpers::date::{DateTime, Duration as Dur, Local, Utc};

use std::env;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time;
use tungstenite::protocol::Message;

pub async fn init(sessions: &mut Sessions, addr: SocketAddr) {
    let mut sessions = sessions.clone();
    let hb_interval = env::var("HEARTBEAT_INTERVAL")
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let mut interval = time::interval(Duration::from_millis(hb_interval));

    tokio::spawn(async move {
        loop {
            interval.tick().await;
            message::find_and_send(&mut sessions, &addr, Message::Ping("".as_bytes().to_vec()))
                .await;
        }
    });
}

pub async fn check(sessions: &Sessions, addr: SocketAddr) {
    let mut sessions = sessions.clone();

    let hb_client_timeout = env::var("MSG_TIMEOUT").unwrap().parse::<u64>().unwrap();

    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(hb_client_timeout));

        loop {
            let hb_timeout: DateTime<Local> =
                Local::now() - Dur::microseconds(hb_client_timeout as i64);

            interval.tick().await;

            log::info!("Checking HB for {addr}");
            handlers::session::find(&mut sessions, &addr, |session| {
                let last_ping = session.last_ping;
                if last_ping < hb_timeout {
                    session.update_client_status(SessionStatus::Down);
                    log::error!("Ping not received from {addr}");
                }
            })
            .await;
        }
    });
}

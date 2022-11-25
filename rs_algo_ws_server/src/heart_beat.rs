use crate::handlers::session::{SessionStatus, Sessions};
use crate::handlers::{self};
use crate::message;

use rs_algo_shared::helpers::date::{DateTime, Duration as Dur, Local};

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
    check(&mut sessions).await;

    tokio::spawn(async move {
        loop {
            interval.tick().await;
            message::broadcast(&mut sessions, &addr, Message::Ping("".as_bytes().to_vec())).await;
        }
    });
}

pub async fn check(sessions: &mut Sessions) {
    let mut sessions = sessions.clone();

    let hb_client_timeout = env::var("MSG_TIMEOUT").unwrap().parse::<u64>().unwrap();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(hb_client_timeout));

        loop {
            interval.tick().await;

            let session_guard = sessions.lock().await.clone();
            let hb_timeout: DateTime<Local> =
                Local::now() - Dur::microseconds((hb_client_timeout * 1000) as i64);

            for (addr, session) in session_guard.into_iter() {
                let last_ping = session.last_ping;

                if last_ping < hb_timeout {
                    handlers::session::find(&mut sessions, &addr, |session| {
                        *session = session.update_status(SessionStatus::Down).clone();
                    })
                    .await;
                    log::error!("Client {addr} not responding",);
                } else {
                    //log::info!("Client {addr} Ok");
                }
            }
        }
    });
}

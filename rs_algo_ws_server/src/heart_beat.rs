use crate::{handlers::session::Sessions, message};

use rs_algo_shared::{
    helpers::date::{DateTime, Duration as Dur, Local},
    ws::message::ReconnectOptions,
};

use futures::future;
use std::time::Duration;
use std::{env, net::SocketAddr};
use tokio::time;

pub async fn init(sessions: &mut Sessions) {
    let sessions = sessions.clone();
    let heartbeat_interval = env::var("HEARTBEAT_INTERVAL")
        .unwrap()
        .parse::<u64>()
        .unwrap();

    let last_data_timeout = env::var("LAST_DATA_TIMEOUT")
        .unwrap()
        .parse::<u64>()
        .unwrap();

    let mut hb_interval = time::interval(Duration::from_secs(heartbeat_interval));

    tokio::spawn(async move {
        loop {
            hb_interval.tick().await;

            let mut sessions_to_remove: Vec<SocketAddr> = vec![];

            let hb_timeout: DateTime<Local> =
                Local::now() - Dur::seconds((last_data_timeout) as i64);

            let mut futures = vec![];

            {
                let mut session_guard = sessions.lock().await;
                let len = session_guard.len();
                log::info!("Active sessions: {:?}", len);

                for (addr, session) in session_guard.iter_mut() {
                    let last_data = session.last_data;
                    let bot_name = session.bot_name();

                    if last_data < hb_timeout && session.symbol() != "init" {
                        let is_open = session.market_hours.is_open();

                        match is_open {
                            true => {
                                log::info!("{:?} session KO while market is open.", &bot_name);

                                let session_clone = session.clone();
                                sessions_to_remove.push(*addr);

                                let future = async move {
                                    message::send_reconnect(
                                        &session_clone,
                                        ReconnectOptions { clean_data: true },
                                    )
                                    .await;
                                };

                                futures.push(future);
                            }
                            false => {}
                        }
                    }
                }
            }

            future::join_all(futures).await;

            {
                let mut session_guard = sessions.lock().await;

                for addr in sessions_to_remove.iter() {
                    if let Some(session) = session_guard.remove(addr) {
                        log::warn!("Session {:?} {} destroyed!", session.bot_name(), addr);
                    } else {
                        log::error!("Session {} not found.", addr);
                    }
                }
            }
        }
    });
}

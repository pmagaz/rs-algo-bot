use crate::handlers::session::Sessions;
use crate::handlers::{self};
use crate::message;

use rs_algo_shared::helpers::date::{DateTime, Duration as Dur, Local};

use std::env;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time;

pub async fn init2(_sessions: &mut Sessions, _addr: SocketAddr) {
    //let mut sessions = sessions.clone();
    //let hb_interval = env::var("HEARTBEAT_INTERVAL")
    // .unwrap()
    // .parse::<u64>()
    // .unwrap();
    //let mut interval = time::interval(Duration::from_millis(hb_interval));
    //check(&mut sessions, &addr).await;

    // tokio::spawn(async move {
    //     loop {
    //         interval.tick().await;
    //         message::broadcast(&mut sessions, &addr, Message::Ping("".as_bytes().to_vec())).await;
    //     }
    // });
}

pub async fn init(sessions: &mut Sessions, _add: &SocketAddr) {
    let mut sessions = sessions.clone();

    let last_data_timeout = env::var("LAST_DATA_TIMEOUT")
        .unwrap()
        .parse::<u64>()
        .unwrap();

    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(last_data_timeout));

        loop {
            interval.tick().await;

            let session_guard = sessions.lock().await.clone();
            let hb_timeout: DateTime<Local> =
                Local::now() - Dur::seconds((last_data_timeout) as i64);

            for (addr, session) in session_guard.into_iter() {
                let last_data = session.last_data;

                if last_data < hb_timeout {
                    handlers::session::find(&mut sessions, &addr, |session| {
                        let is_open = session.market_hours.is_open();

                        log::warn!(
                            "Session {:?} last data not received since {:?}",
                            addr,
                            session.last_data
                        );
                        match is_open {
                            true => {
                                log::info!("{:?} session KO. Market open.", addr,);
                                tokio::spawn({
                                    let session = session.clone();
                                    async move {
                                        message::send_reconnect(&session).await;
                                    }
                                });
                            }
                            false => {
                                log::info!("{:?} session Ok. Market not open.", addr,);
                            }
                        }
                    })
                    .await;
                } else {
                    log::info!(
                        "{:?} session Ok. Last data received: {:?}",
                        addr,
                        session.last_data
                    );
                }
            }
        }
    });
}

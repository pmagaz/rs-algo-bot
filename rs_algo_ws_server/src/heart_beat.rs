use crate::handlers::session::Sessions;

use chrono::Timelike;
use rs_algo_shared::helpers::date::{self, DateTime, Duration as Dur, Local};

use std::env;
use std::time::Duration;
use tokio::time::{self};

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
            let mut session_guard = sessions.lock().await;
            let len = session_guard.len();
            log::info!("HeartBeat active sessions: {}", len);

            let hb_timeout: DateTime<Local> =
                Local::now() - Dur::seconds((last_data_timeout) as i64);

            for (_addr, session) in session_guard.iter_mut() {
                let last_data = session.last_data;
                let bot_name = session.bot_name();

                log::info!(
                    "HeartBeat {:?}  {:?} -> {:?}",
                    &bot_name,
                    last_data,
                    hb_timeout
                );

                if last_data < hb_timeout {
                    let is_open = session.market_hours.is_open();
                    let current_date = Local::now();
                    let current_hours = current_date.hour();
                    let week_day = date::get_week_day(current_date) as u32;

                    log::warn!("Session {:?} last  {:?} ", len, session.clone(),);

                    log::warn!(
                        "Market {:?}",
                        (
                            is_open,
                            current_hours,
                            week_day,
                            session.market_hours.clone()
                        )
                    );

                    match is_open {
                        true => {
                            log::info!("{:?} session KO. Market open.", &bot_name);
                            tokio::spawn({
                                let session = session.clone();
                                async move {
                                    // message::send_reconnect(
                                    //     &session,
                                    //     ReconnectOptions { clean_data: true },
                                    // )
                                    // .await;
                                }
                            });
                        }
                        false => {
                            log::info!("{:?} session Ok. Market not open.", &bot_name);
                        }
                    }
                }
            }
            drop(session_guard);
        }
    });
}

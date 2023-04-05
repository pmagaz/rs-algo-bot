use crate::handlers::session::Sessions;
use crate::handlers::{self};

use chrono::Timelike;
use rs_algo_shared::helpers::date::{self, DateTime, Duration as Dur, Local};

use std::env;
use std::time::Duration;
use tokio::time::{self, sleep};

pub async fn init(sessions: &mut Sessions) {
    let mut sessions = sessions.clone();

    let last_data_timeout = env::var("LAST_DATA_TIMEOUT")
        .unwrap()
        .parse::<u64>()
        .unwrap();

    sleep(Duration::from_secs(10)).await;

    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(last_data_timeout));

        loop {
            log::info!("HeartBeat tick");

            interval.tick().await;

            let session_guard = sessions.lock().await.clone();
            let len = session_guard.len();
            let hb_timeout: DateTime<Local> =
                Local::now() - Dur::seconds((last_data_timeout) as i64);

            for (addr, session) in session_guard.into_iter() {
                let last_data = session.last_data;
                let bot_name = session.bot_name();
                log::info!(
                    "HeartBeat {:?}  {:?} -> {:?}",
                    &bot_name,
                    last_data,
                    hb_timeout
                );

                if last_data < hb_timeout {
                    handlers::session::find(&mut sessions, &addr, |session| {
                        let is_open = session.market_hours.is_open();
                        let current_date = Local::now();
                        let current_hours = current_date.hour();
                        let week_day = date::get_week_day(current_date) as u32;

                        //log::warn!("Session {:?} last  {:?} ", len, session.clone(),);

                        // log::warn!(
                        //     "Market {:?}",
                        //     (
                        //         is_open,
                        //         current_hours,
                        //         week_day,
                        //         session.market_hours.clone()
                        //     )
                        // );

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
                    })
                    .await;
                }
            }
        }
    });
}

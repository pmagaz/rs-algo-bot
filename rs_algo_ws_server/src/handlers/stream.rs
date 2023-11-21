use crate::handlers::session::Session;
use crate::message;
use chrono::{Datelike, TimeZone};
pub use rs_algo_shared::broker::BrokerStream;
use rs_algo_shared::broker::{xtb_stream::*, VEC_DOHLC};

use futures_util::StreamExt;
use rs_algo_shared::helpers::date::{self, DateTime, Local, Timelike};
use rs_algo_shared::helpers::http::{request, HttpMethod};
use rs_algo_shared::models::mode::ExecutionMode;
use rs_algo_shared::models::time_frame::TimeFrame;
use rs_algo_shared::ws::message::ResponseBody;
use rs_algo_shared::ws::message::ResponseType;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tungstenite::Message;
pub fn listen<BK>(broker: Arc<Mutex<BK>>, session: Session)
where
    BK: BrokerStream + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<()>(1);
    let symbol = session.symbol();

    tokio::spawn({
        async move {
            /* TEMPORARY WORKARROUND */

            let username = &env::var("BROKER_USERNAME").unwrap();
            let password = &env::var("BROKER_PASSWORD").unwrap();

            //LECHES
            let num_bars = env::var("NUM_BARS_TEST").unwrap().parse::<i64>().unwrap();
            let time_frame = TimeFrame::new("M1");
            let time_frame_from = Local::now() - date::Duration::days(30);
            let time_frame_to = Local::now(); //time_frame_from + date::Duration::days(1);

            let time_frame_number = time_frame.to_number();
            let symbol = session.symbol.clone();
            let historic_data_url = &format!(
                "{}{}/{}/{}",
                env::var("BACKEND_BACKTEST_HISTORIC_ENDPOINT").unwrap(),
                symbol,
                time_frame,
                10000
            );

            let data: VEC_DOHLC =
                request(&historic_data_url, &String::from("all"), HttpMethod::Get)
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();

            let mut counter: usize = 0;
            let sleep_time = 100;

            log::error!("Total data {:?}", data.len());
            for item in data.iter().skip(4) {
                let (date, open, high, low, close, volume) = item;

                let res = ResponseBody {
                    response: ResponseType::SubscribeStream,
                    payload: Some((date, open, high, low, close, volume)),
                };

                let instrument_string =
                    serde_json::to_string(&res).expect("Failed to serialize struct to JSON!");

                let msg = Message::Text(instrument_string);
                match message::send(&session, msg).await {
                    Err(_) => {
                        log::error!("Can't send stream data to {:?}", session.bot_name());
                        tx.send(()).await.unwrap();
                    }
                    _ => (),
                };

                counter = counter + 1;
                log::info!("{:?}", (counter, sleep_time));
                sleep(Duration::from_millis(sleep_time)).await;
            }
        }
    });
}

use crate::handlers::session::Session;
use crate::{handlers, message};
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
            let time_frame: rs_algo_shared::models::time_frame::TimeFrameType =
                TimeFrame::new("M1");

            let symbol = &session.symbol;
            let limit = env::var("LIMIT_HISTORIC_DATA")
                .unwrap_or_else(|_| "0".to_string())
                .parse::<i64>()
                .unwrap_or(0);

            let mut counter: usize = 0;
            let sleep_time = 25;

            let data: VEC_DOHLC = handlers::historic::get_historic_data(symbol, &time_frame, limit)
                .await
                .unwrap();

            log::info!("Total historic {} data {:?}", symbol, data.len());

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

                sleep(Duration::from_millis(sleep_time)).await;
            }
        }
    });
}

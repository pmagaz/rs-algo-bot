use crate::handlers::session::Session;
use crate::message;
use rs_algo_shared::broker::xtb_stream::*;
pub use rs_algo_shared::broker::BrokerStream;
use rs_algo_shared::helpers::date::Local;

use futures_util::StreamExt;
use rs_algo_shared::models::mode::ExecutionMode;
use rs_algo_shared::models::time_frame::TimeFrame;
use rs_algo_shared::ws::message::InstrumentData;
use rs_algo_shared::ws::message::ReconnectOptions;
use rs_algo_shared::ws::message::ResponseBody;
use rs_algo_shared::ws::message::ResponseType;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time;
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
            let num_bars = env::var("NUM_BARS").unwrap().parse::<i64>().unwrap();
            let time_frame = TimeFrame::new("M1");
            let time_frame_from =
                TimeFrame::get_starting_bar(num_bars, &time_frame, &ExecutionMode::Bot);
            let time_frame_number = time_frame.to_number();
            let symbol = session.symbol.clone();
            let mut broker_stream = Xtb::new().await;
            broker_stream.login(username, password).await.unwrap();
            let res = broker_stream
                .get_instrument_data(
                    &symbol,
                    time_frame_number as usize,
                    time_frame_from.timestamp(),
                )
                .await
                .unwrap();

            broker_stream.disconnect().await.unwrap();

            let data = res.payload.unwrap().data;
            for item in data.iter().skip(4) {
                let (date, open, high, low, close, volume) = item;

                let res = ResponseBody {
                    response: ResponseType::SubscribeStream,
                    payload: Some((date, open, high, low, close, volume)),
                };

                let instrument_string =
                    serde_json::to_string(&res).expect("Failed to serialize struct to JSON");

                let msg = Message::Text(instrument_string);
                match message::send(&session, msg).await {
                    Err(_) => {
                        log::error!("Can't send stream data to {:?}", session.bot_name());
                        tx.send(()).await.unwrap();
                    }
                    _ => (),
                };
                sleep(Duration::from_millis(50)).await;
            }
        }
    });
}

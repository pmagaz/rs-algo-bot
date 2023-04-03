use crate::handlers::session::Session;
use crate::{handlers, message};
use rs_algo_shared::broker::xtb_stream::*;
pub use rs_algo_shared::broker::BrokerStream;
use rs_algo_shared::helpers::date::Local;

use futures_util::StreamExt;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time;
use tungstenite::Message;

pub fn listen<BK>(broker: Arc<Mutex<BK>>, session: Session, symbol: String)
where
    BK: BrokerStream + Send + 'static,
{
    tokio::spawn({
        async move {
            /* TEMPORARY WORKARROUND */
            let username = &env::var("BROKER_USERNAME").unwrap();
            let password = &env::var("BROKER_PASSWORD").unwrap();
            let keepalive_interval = env::var("KEEPALIVE_INTERVAL")
                .unwrap()
                .parse::<u64>()
                .unwrap();

            let mut broker_stream = Xtb::new().await;
            broker_stream.login(username, password).await.unwrap();
            broker_stream
                .get_instrument_data(&symbol, 1, Local::now().timestamp())
                .await
                .unwrap();
            broker_stream.subscribe_stream(&symbol).await.unwrap();
            let mut interval = time::interval(Duration::from_millis(keepalive_interval));

            loop {
                tokio::select! {
                stream = broker_stream.get_stream().await.next() => {
                        match stream {
                            Some(data) => {
                                 match data {
                                    Ok(msg) => {
                                   if msg.is_text() {
                                           let txt = BK::parse_stream_data(msg).await;
                                           match txt {
                                               Some(txt) =>  message::send(&session, Message::Text(txt)).await,
                                               None => ()
                                          };

                                    } else if msg.is_close() {
                                        message::send_reconnect(&session).await;
                                        log::error!("Streaming close msg!");
                                        break;
                                    }
                                    },
                                    Err(err) => {
                                        message::send_reconnect(&session).await;
                                        log::error!("handling stream {:?}", (err, &session));
                                        break;
                                    }
                                };
                            }
                            None => ()
                        }
                    }
                    _ = interval.tick() => {
                        broker_stream.keepalive_ping().await.unwrap();
                        let mut guard = broker.lock().await;
                        guard.keepalive_ping().await.unwrap();
                    }
                }
            }
        }
    });
}

use crate::handlers::session::Session;
use crate::message;
use rs_algo_shared::broker::xtb_stream::*;
pub use rs_algo_shared::broker::BrokerStream;
use rs_algo_shared::helpers::date::Local;

use futures_util::StreamExt;
use rs_algo_shared::ws::message::ReconnectOptions;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time;
use tungstenite::Message;

pub fn listen<BK>(broker: Arc<Mutex<BK>>, session: Session)
where
    BK: BrokerStream + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<()>(1);

    tokio::spawn({
        async move {
            /* TEMPORARY WORKAROUND */
            let username = &env::var("BROKER_USERNAME").unwrap();
            let password = &env::var("BROKER_PASSWORD").unwrap();
            let keepalive_interval = env::var("KEEPALIVE_INTERVAL")
                .unwrap()
                .parse::<u64>()
                .unwrap();

            let symbol = session.symbol.clone();
            let mut broker_stream = Xtb::new().await;

            broker_stream.login(username, password).await.unwrap();
            broker_stream
                .get_instrument_data(&symbol, 1, Local::now().timestamp())
                .await
                .unwrap();
            broker_stream.subscribe_stream(&symbol).await.unwrap();
            broker_stream.subscribe_tick_prices(&symbol).await.unwrap();

            let mut msg_sent = "".to_owned();
            let mut interval = time::interval(Duration::from_millis(keepalive_interval));

            loop {
                tokio::select! {
                    stream = broker_stream.get_stream().await.next() => {
                        match stream {
                            Some(data) => {
                                match data {
                                    Ok(msg) => {
                                        if msg.is_text() {
                                            let parsed = BK::parse_stream_data(msg).await;
                                            match parsed {
                                                Some(txt) => {
                                                    if &msg_sent != &txt {
                                                        match message::send(&session, Message::Text(txt.clone())).await {
                                                            Ok(_) => msg_sent = txt,
                                                            Err(_) => {
                                                                log::error!("Can't send stream data to {:?}", session.bot_name());
                                                                tx.send(()).await.unwrap();
                                                            }
                                                        }
                                                    }
                                                }
                                                None => ()
                                            };
                                        } else if msg.is_close() {
                                            log::error!("Stream closed by broker");
                                            message::send_reconnect(&session, ReconnectOptions { clean_data: true }).await;
                                            tx.send(()).await.unwrap();
                                        }
                                    },
                                    Err(err) => {
                                        log::error!("Stream error {:?}", (err, &session));
                                        message::send_reconnect(&session, ReconnectOptions { clean_data: true }).await;
                                        tx.send(()).await.unwrap();
                                    }
                                };
                            }
                            None => {
                                log::error!("No stream data");
                                message::send_reconnect(&session, ReconnectOptions { clean_data: true }).await;
                                tx.send(()).await.unwrap();
                            }
                        }
                    }
                    _ = interval.tick() => {
                        broker_stream.keepalive_ping().await.unwrap();
                        let mut guard = broker.lock().await;
                        guard.keepalive_ping().await.unwrap();
                    }
                    _ = rx.recv() => {
                        log::warn!("Stream {} stopped!", session.bot_name());
                        break;
                    }
                }
            }
        }
    });
}

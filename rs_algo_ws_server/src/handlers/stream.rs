use crate::handlers::session::Session;
use crate::message;

use futures_util::StreamExt;
use rs_algo_shared::broker::xtb_stream::*;
pub use rs_algo_shared::broker::BrokerStream;
use std::sync::Arc;
use std::time::Duration;
use std::{collections::HashMap, env, net::SocketAddr};
use tokio::sync::Mutex;
use tokio::time;
use tungstenite::Message;
pub fn listen<BK>(
    broker: Arc<Mutex<BK>>,
    //sessions: &mut Sessions,
    session: Session,
    addr: &SocketAddr,
    symbol: String,
    //mut callback: F,
) where
    BK: BrokerStream + Send + 'static,
    // F: 'static + Send + FnMut(Message) -> T,
    // T: Future<Output = Result<()>> + Send + 'static,
{
    tokio::spawn({
        async move {
            /* TEMPORARY WORKARROUND */
            let username = &env::var("BROKER_USERNAME").unwrap();
            let password = &env::var("BROKER_PASSWORD").unwrap();
            let keepalive_interval = env::var("BROKER_PASSWORD").unwrap().parse::<u64>().unwrap();
            let mut broker_stream = Xtb::new().await;
            broker_stream.login(username, password).await.unwrap();
            broker_stream.subscribe_stream(&symbol).await.unwrap();
            let mut interval2 = time::interval(Duration::from_millis(keepalive_interval));

            loop {
                tokio::select! {
                stream = broker_stream.get_stream().await.next() => {
                        match stream {
                            Some(data) => {
                                 match data {
                                    Ok(msg) => {
                                       if msg.is_text() || msg.is_binary() {
                                           let txt = BK::parse_stream_data(msg).await;
                                           match txt {
                                               Some(txt) =>  message::send(&session, Message::Text(txt)).await,
                                               None => ()
                                           };
                                } else if msg.is_close() {
                                    log::error!("MSG close!");
                                    break;
                                }
                                    },
                                    Err(err) => log::error!("{}", err)
                                };
                            }
                            None => ()
                        }
                    }
                    _ = interval2.tick() => {
                        broker_stream.keepalive_ping().await.unwrap();
                        broker.lock().await.keepalive_ping().await.unwrap();
                    }
                }
            }
        }
    });
}

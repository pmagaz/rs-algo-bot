use crate::db;
use crate::error::Result;
use crate::message;
use crate::session::Sessions;

use futures_util::{Future, SinkExt, StreamExt};
pub use rs_algo_shared::broker::BrokerStream;
use std::time::Duration;
use std::{env, net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tokio::time;
use tungstenite::Message;

pub fn listen<BK>(
    broker: Arc<Mutex<BK>>,
    sessions: &mut Sessions,
    addr: &SocketAddr,
    symbol: String,
    //mut callback: F,
) -> ()
where
    BK: BrokerStream + Send + 'static,
    // F: 'static + Send + FnMut(Message) -> T,
    // T: Future<Output = Result<()>> + Send + 'static,
{
    // let instrument = db::instrument::find_by_symbol(db_client, "aaaa")
    //     .await
    //     .unwrap();

    tokio::spawn({
        let sessions = Arc::clone(&sessions);
        let addr = addr.clone();
        async move {
            let mut guard = broker.lock().await;
            guard.get_instrument_streaming(&symbol, 1, 2).await.unwrap();
            let mut interval = time::interval(Duration::from_millis(200));
            let mut sessions = Arc::clone(&sessions);
            let read_stream = guard.get_stream().await;

            loop {
                tokio::select! {
                    msg = read_stream.next() => {
                        match msg {
                            Some(msg) => {
                                let msg = msg.unwrap();
                                log::info!("Msg from Broker received");

                                if msg.is_text() || msg.is_binary() {
                                    //tokio::spawn(callback(msg));
                                    message::send(&mut sessions, &addr, msg).await;
                                } else if msg.is_close() {
                                    log::error!("MSG close!");
                                    break;
                                }
                            }
                            None => {
                                log::error!("MSG none!");
                                break
                            }
                        }
                    }
                    _ = interval.tick() => {
                        //self::send(&mut sessions, &addr, Message::Ping(b"".to_vec())).await;
                    }
                }
            }
        }
    });
}

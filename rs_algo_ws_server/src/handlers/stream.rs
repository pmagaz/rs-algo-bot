use crate::db;
use crate::error::Result;
use crate::handlers::session::{Session, Sessions};
use crate::message;

use futures_util::{Future, SinkExt, StreamExt};
pub use rs_algo_shared::broker::BrokerStream;
use std::time::Duration;
use std::{env, net::SocketAddr, sync::Arc};
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
        //let sessions = Arc::clone(&sessions);
        let addr = addr.clone();
        async move {
            let mut guard = broker.lock().await;
            guard.subscribe_stream(&symbol, 5000, 2).await.unwrap();
            let mut interval = time::interval(Duration::from_millis(200));
            //let mut sessions = Arc::clone(&sessions);
            let read_stream = guard.get_stream().await;
            loop {
                tokio::select! {
                    msg = read_stream.next() => {
                        match msg {
                            Some(msg) => {
                                let msg = msg.unwrap();
                                log::info!("Msg from Broker received");

                                if msg.is_text() || msg.is_binary() {
                                    let txt = BK::parse_stream_data(msg).await.unwrap();
                                    message::send(&session, Message::Text(txt)).await;
                                    //message::send(&mut sessions, &addr, Message::Text(txt)).await;
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
                    }
                }
            }
        }
    });
}

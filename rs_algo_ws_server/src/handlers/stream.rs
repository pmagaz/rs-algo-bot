use crate::handlers::session::Session;
use crate::message;

use futures_util::StreamExt;
pub use rs_algo_shared::broker::BrokerStream;
use std::time::Duration;
use std::{net::SocketAddr, sync::Arc};
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
    // let instrument = db::instrument::find_by_symbol(db_client, "aaaa")
    //     .await
    //     .unwrap();

    //let cloned = Arc::new(Mutex::new(&broker));
    let mut interval2 = time::interval(Duration::from_millis(1000));
    let cloned = Arc::clone(&broker);
    // tokio::spawn({
    //     async move {
    //         let cloned = Arc::clone(&broker);

    //         loop {
    //             interval2.tick().await;
    //             println!("2222222");

    //             let cloned = Arc::clone(&cloned);
    //             let lehes = match cloned.try_lock() {
    //                 Ok(a) => println!("111111"),
    //                 Err(a) => println!("errr {:?}", a),
    //             };
    //             // let mut guard = cloned.try_lock().unwrap();
    //             // guard.keepalive_ping().await.unwrap();
    //         }
    //     }
    // });

    tokio::spawn({
        let cloned = Arc::clone(&broker);
        async move {
            // let cloned = Arc::clone(&cloned);

            let mut guard = broker.lock().await;
            // let leches = Arc::new(Mutex::new(&guard));
            // let leches = leches.clone();
            //     // //let cloned = Arc::clone(&broker);
            //    // tokio::spawn({
            //         let cloned = Arc::clone(&cloned);
            //         async move {
            //             let cloned = Arc::clone(&cloned);

            //             loop {
            //                 let cloned = Arc::clone(&cloned);

            //                 //interval2.tick().await;
            //                 //let mut guard = cloned.lock_owned().await;
            //                 guard.keepalive_ping().await.unwrap();
            //             }
            //         }
            //    // });

            guard.subscribe_stream(&symbol).await.unwrap();
            let mut interval = time::interval(Duration::from_millis(250));
            let mut interval2 = time::interval(Duration::from_millis(5250));
            let read_stream = guard.get_stream().await;

            loop {
                tokio::select! {
                stream = read_stream.next() => {
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
                    _ = interval.tick() => {
                    }
                }
                {
                    println!("111111111");
                    interval2.tick().await;
                    // let lehes = match cloned.try_lock() {
                    //     Ok(a) => println!("111111"),
                    //     Err(a) => println!("errrrrrrrrr {:?}", a),
                    // };
                    //let guard = leches.lock().await;
                    //guard.keepalive_ping().await.unwrap();
                    //broker.lock().await.keepalive_ping().await.unwrap();
                }
            }
        }
    });
}

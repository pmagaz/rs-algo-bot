use crate::error::RsAlgoErrorKind;
use crate::handlers::session::Session;
use crate::message;
pub use rs_algo_shared::broker::BrokerStream;
use rs_algo_shared::helpers::date::Local;
use rs_algo_shared::{broker::xtb_stream::*, models::environment};

use futures_util::StreamExt;
use rs_algo_shared::ws::message::ReconnectOptions;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tokio::time;
use tungstenite::{Error, Message};

async fn initialize_broker_stream(symbol: &str) -> Result<Xtb, RsAlgoErrorKind> {
    let env = environment::from_str(&env::var("ENV").unwrap());
    let username = env::var("BROKER_USERNAME").map_err(|_| RsAlgoErrorKind::EnvVarNotFound)?;
    let password = env::var("BROKER_PASSWORD").map_err(|_| RsAlgoErrorKind::EnvVarNotFound)?;

    let symbol = symbol.to_string();
    let mut broker_stream = Xtb::new().await;

    broker_stream.login(&username, &password).await.unwrap();
    broker_stream
        .get_instrument_data(&symbol, 1, Local::now().timestamp())
        .await
        .unwrap();

    broker_stream.subscribe_stream(&symbol).await.unwrap();
    broker_stream.subscribe_tick_prices(&symbol).await.unwrap();

    if env.is_prod() {
        broker_stream.subscribe_trades(&symbol).await.unwrap();
    }

    Ok(broker_stream)
}

pub async fn handle_strean_data<BK: BrokerStream + Send + 'static>(
    tx: &Sender<()>,
    session: &Session,
    data: Result<Message, Error>,
) {
    match data {
        Ok(msg) => {
            if msg.is_text() {
                let symbol = session.symbol.as_ref();
                let strategy_name = session.strategy.as_ref();
                let parsed = BK::parse_stream_data(msg, &symbol, &strategy_name).await;
                match parsed {
                    Some(txt) => match message::send(session, Message::Text(txt)).await {
                        Ok(_) => (),
                        Err(_) => {
                            log::error!("Can't send stream data to {:?}", session.bot_name());
                            tx.send(()).await.unwrap();
                        }
                    },
                    None => (),
                };
            } else if msg.is_close() {
                log::error!("Stream closed by broker");
                message::send_reconnect(session, ReconnectOptions { clean_data: true }).await;
                tx.send(()).await.unwrap();
            }
        }
        Err(err) => {
            log::error!("Stream error {:?}", (err, &session));
            message::send_reconnect(session, ReconnectOptions { clean_data: true }).await;
            tx.send(()).await.unwrap();
        }
    }
}

pub fn listen<BK>(broker: Arc<Mutex<BK>>, session: Session)
where
    BK: BrokerStream + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<()>(1);

    tokio::spawn({
        async move {
            let keepalive_interval = env::var("KEEPALIVE_INTERVAL")
                .map_err(|_| RsAlgoErrorKind::EnvVarNotFound)
                .unwrap()
                .parse::<u64>()
                .unwrap();

            let symbol = session.symbol.as_ref();
            let mut broker_stream = initialize_broker_stream(&symbol).await.unwrap();
            let mut interval = time::interval(Duration::from_millis(keepalive_interval));

            loop {
                tokio::select! {
                    stream = broker_stream.get_stream().await.next() => {
                        match stream {
                            Some(data) => {
                                handle_strean_data::<BK>(&tx, &session, data).await;
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

use crate::error::RsAlgoErrorKind;
use crate::handlers::session::Session;
use crate::message;
use rs_algo_shared::broker::BrokerStream;
use rs_algo_shared::ws::message::ReconnectOptions;

use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time;
use tungstenite::Message;

pub fn listen<BK: BrokerStream + Send + 'static>(broker: Arc<Mutex<BK>>, session: Session) {
    tokio::spawn(async move {
        let keepalive_ms = env::var("KEEPALIVE_INTERVAL")
            .map_err(|_| RsAlgoErrorKind::EnvVarNotFound)
            .unwrap()
            .parse::<u64>()
            .unwrap();

        let symbol = session.symbol.clone();
        let strategy_name = session.strategy.clone();

        let mut stream_rx = {
            let mut guard = broker.lock().await;
            guard
                .subscribe_stream(&symbol, &strategy_name)
                .await
                .unwrap()
        };

        let mut interval = time::interval(Duration::from_millis(keepalive_ms));

        loop {
            tokio::select! {
                msg = stream_rx.recv() => {
                    match msg {
                        Some(txt) => {
                            if message::send(&session, Message::Text(txt)).await.is_err() {
                                tracing::error!(
                                    "Stream: failed to forward tick to '{}' — disconnecting",
                                    session.bot_name()
                                );
                                message::send_reconnect(&session, ReconnectOptions { clean_data: false }).await;
                                break;
                            }
                        }
                        None => {
                            tracing::error!(
                                "Stream: broker channel closed for '{}' — reconnecting",
                                session.bot_name()
                            );
                            message::send_reconnect(&session, ReconnectOptions { clean_data: true }).await;
                            break;
                        }
                    }
                }
                _ = interval.tick() => {
                    broker.lock().await.keepalive_ping().await.unwrap();
                }
            }
        }
    });
}

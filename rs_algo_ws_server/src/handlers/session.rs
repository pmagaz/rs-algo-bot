use crate::db;
use rs_algo_shared::helpers::date::*;
use rs_algo_shared::helpers::uuid::*;
use rs_algo_shared::models::strategy::*;
use rs_algo_shared::models::time_frame::*;
use rs_algo_shared::ws::message::*;

use futures::Future;
use futures_channel::mpsc::UnboundedSender;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tungstenite::protocol::Message;

#[derive(Debug, Clone)]
pub enum SessionStatus {
    Up,
    Down,
    Connecting,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    #[serde(rename = "_id")]
    pub id: Uuid,
    pub strategy: String,
    pub strategy_type: StrategyType,
    pub symbol: String,
    pub time_frame: TimeFrameType,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub session_id: Uuid,
    pub recipient: UnboundedSender<Message>,
    pub symbol: String,
    pub strategy: String,
    pub time_frame: TimeFrameType,
    pub strategy_type: StrategyType,
    pub started: DateTime<Local>,
    pub last_ping: DateTime<Local>,
    pub client_status: SessionStatus,
}

pub type Sessions = Arc<Mutex<HashMap<SocketAddr, Session>>>;

impl Session {
    pub fn new(recipient: UnboundedSender<Message>) -> Self {
        Self {
            session_id: mongodb::bson::uuid::Uuid::new(),
            recipient,
            symbol: "init".to_string(),
            strategy: "init".to_string(),
            time_frame: TimeFrameType::M1,
            strategy_type: StrategyType::OnlyLong,
            started: Local::now(),
            last_ping: Local::now(),
            client_status: SessionStatus::Up,
        }
    }

    pub fn update_name(&mut self, symbol: &str, strategy: &str) -> &Self {
        self.symbol = symbol.to_owned();
        self.strategy = strategy.to_owned();
        self
    }

    pub fn update_ping(&mut self) -> &Self {
        self.last_ping = Local::now();
        self
    }

    pub fn update_data(&mut self, data: SessionData) -> &Self {
        self.session_id = data.id;
        self.symbol = data.symbol;
        self.time_frame = data.time_frame;
        self.strategy = data.strategy;
        self.strategy_type = data.strategy_type;
        self.client_status = SessionStatus::Up;
        self
    }

    pub fn update_status(&mut self, status: SessionStatus) -> &Self {
        self.client_status = status;
        self
    }
}

pub async fn find_async<'a, C, F>(sessions: &Sessions, addr: &SocketAddr, callback: C)
where
    C: Fn(&mut Session) -> F,
    F: Future<Output = ()>,
{
    let mut sessions = sessions.lock().await;
    match sessions.get_mut(addr) {
        Some(session) => callback(session),
        None => panic!("Session not found!"),
    };
}

pub async fn find<'a, F>(sessions: &'a mut Sessions, addr: &SocketAddr, callback: F)
where
    F: Send + FnOnce(&mut Session),
    // F: 'static + Send + FnMut(Message) -> T,
    // T: Future<Output = Result<()>> + Send + 'static,
{
    let mut sessions = sessions.lock().await;
    match sessions.get_mut(addr) {
        Some(session) => callback(&mut *session),
        None => panic!("Session not found!"),
    };
}

pub async fn create<'a>(
    sessions: &'a mut Sessions,
    addr: &SocketAddr,
    recipient: UnboundedSender<tungstenite::Message>,
) -> Session {
    let session = Session::new(recipient);

    sessions.lock().await.insert(*addr, session.clone());

    let msg: ResponseBody<String> = ResponseBody {
        response: ResponseType::Connected,
        payload: None,
    };

    let msg: String = serde_json::to_string(&msg).unwrap();
    session
        .recipient
        .unbounded_send(Message::Text(msg))
        .unwrap();
    session
}

pub async fn update_db_session(data: &SessionData, db_client: &mongodb::Client) {
    db::session::upsert(db_client, data).await.unwrap();
}

pub async fn destroy<'a>(sessions: &'a mut Sessions, addr: &SocketAddr) {
    log::warn!("{} session destroyed", &addr);
    sessions.lock().await.remove(&addr);
}

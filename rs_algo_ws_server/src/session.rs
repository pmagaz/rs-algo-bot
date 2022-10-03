use futures_channel::mpsc::UnboundedSender;
use rs_algo_shared::helpers::date::*;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex, PoisonError},
};
use tungstenite::protocol::Message;

#[derive(Debug, Clone)]
pub enum SessionStatus {
    Up,
    Down,
    Connecting
}

#[derive(Debug, Clone)]
pub struct Session {
    pub recipient: UnboundedSender<Message>,
    pub symbol: String,
    pub strategy: String,
    pub last_ping: DateTime<Local>,
    pub client_status: SessionStatus,
}

pub type Sessions = Arc<Mutex<HashMap<SocketAddr, Session>>>;

impl Session {
    pub fn new(recipient: UnboundedSender<Message>) -> Self {
        Self {
            recipient,
            symbol: "init".to_string(),
            strategy: "init".to_string(),
            last_ping: Local::now(),
            client_status: SessionStatus::Up
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

    pub fn update_client_status(&mut self, status: SessionStatus) -> &Self {
        self.client_status = status;
        self
    }

}


pub fn find_session<'a, F>(sessions: &'a mut Sessions, addr: &SocketAddr, mut callback: F)
where
    F: Send + FnOnce(&mut Session),
{
    let mut sessions = sessions.lock().unwrap_or_else(PoisonError::into_inner);
    match sessions.get_mut(addr) {
        Some(session) => callback(session),
        None => panic!("Session not found!"),
    };
}

pub fn create_session<'a>(sessions: &'a mut Sessions, addr: &SocketAddr, session: Session) {
    sessions.lock().unwrap().insert(*addr, session);
}
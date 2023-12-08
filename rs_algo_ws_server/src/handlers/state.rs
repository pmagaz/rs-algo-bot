use rs_algo_shared::helpers::date::*;








use std::{sync::Arc};
use tokio::sync::Mutex;



use super::session::Sessions2;

#[derive(Debug, Clone)]
pub struct Portfolio {
    initial_value: f64,
    current_value: f64,
    cash_balance: f64,
    profit: f64,
    profit_per: f64,
    profit_factor: f64,
    winrate: f64,
    drawdown: usize,
    won_positions: usize,
    lost_positions: usize,
    last_updated: DateTime<Local>,
}

impl Default for Portfolio {
    fn default() -> Self {
        Portfolio {
            initial_value: 0.0,
            current_value: 0.0,
            cash_balance: 0.0,
            profit: 0.0,
            profit_per: 0.0,
            profit_factor: 0.0,
            winrate: 0.0,
            drawdown: 0,
            won_positions: 0,
            lost_positions: 0,
            last_updated: Local::now(),
        }
    }
}

pub type AppState = Arc<Mutex<State>>;

#[derive(Debug, Clone)]
pub struct State {
    pub sessions: Sessions2,
    pub portfolio: Portfolio,
}

impl State {
    pub fn new() -> AppState {
        let portfolio = Portfolio::default();

        Arc::new(Mutex::new(State {
            portfolio,
            sessions: Sessions2::new(),
        }))
    }
}

// impl Session {
//     pub fn new(recipient: UnboundedSender<Message>) -> Self {
//         Self {
//             session_id: mongodb::bson::uuid::Uuid::new(),
//             recipient,
//             symbol: "init".to_string(),
//             strategy: "init".to_string(),
//             time_frame: TimeFrameType::M1,
//             market_hours: MarketHours::new("init".to_string(), vec![]),
//             strategy_type: StrategyType::OnlyLong,
//             started: Local::now(),
//             last_ping: Local::now(),
//             last_data: Local::now(),
//             client_status: SessionStatus::Up,
//         }
//     }

//     pub fn update_bot_name(&mut self, symbol: &str, time_frame: &str, strategy: &str) -> &Self {
//         self.symbol = symbol.to_owned();
//         self.time_frame = TimeFrame::new(time_frame);
//         self.strategy = strategy.to_owned();
//         self
//     }

//     pub fn update_ping(&mut self) -> &Self {
//         self.last_ping = Local::now();
//         self
//     }

//     pub fn update_last_data(&mut self) -> &Self {
//         self.last_data = Local::now();
//         self
//     }

//     pub fn symbol(&self) -> &String {
//         &self.symbol
//     }

//     pub fn bot_name(&self) -> String {
//         [
//             &self.symbol,
//             "_",
//             &self.time_frame.to_string(),
//             "_",
//             &self.strategy,
//         ]
//         .concat()
//     }

//     pub fn update_market_hours(&mut self, market_hours: MarketHours) -> &mut Self {
//         // log::info!(
//         //     "Updating {} market hours {:?}",
//         //     self.bot_name(),
//         //     market_hours
//         // );
//         self.market_hours = market_hours;
//         self
//     }

//     pub fn update_data(&mut self, data: SessionData) -> &Self {
//         self.session_id = data.id;
//         self.symbol = data.symbol;
//         self.time_frame = data.time_frame;
//         self.strategy = data.strategy;
//         self.strategy_type = data.strategy_type;
//         self.last_data = Local::now();
//         self.client_status = SessionStatus::Up;
//         self
//     }

//     pub fn update_status(&mut self, status: SessionStatus) -> &Self {
//         self.client_status = status;
//         self
//     }
// }

// // pub async fn find_async<'a, C, F>(sessions: &Sessions, addr: &SocketAddr, callback: C)
// // where
// //     C: Fn(&mut Session) -> F,
// //     F: Future<Output = ()>,
// // {
// //     let mut sessions = sessions.lock().await;
// //     match sessions.get_mut(addr) {
// //         Some(session) => callback(session),
// //         None => panic!("Session not found!"),
// //     };
// // }

// pub async fn find<'a, F>(sessions: &'a mut Sessions, addr: &SocketAddr, callback: F)
// where
//     F: Send + FnOnce(&mut Session),
//     // F: 'static + Send + FnMut(Message) -> T,
//     // T: Future<Output = Result<()>> + Send + 'static,
// {
//     let mut sessions_guard = sessions.lock().await;
//     match sessions_guard.get_mut(addr) {
//         Some(session) => callback(&mut *session),
//         None => panic!("Session not found!"),
//     };
// }

// pub async fn create<'a>(
//     sessions: &'a mut Sessions,
//     addr: &SocketAddr,
//     recipient: UnboundedSender<tungstenite::Message>,
// ) -> Session {
//     let session = Session::new(recipient);

//     {
//         sessions.lock().await.insert(*addr, session.clone());
//     }

//     log::warn!("Session {:?} created!", (addr, session.bot_name()));

//     let msg: ResponseBody<String> = ResponseBody {
//         response: ResponseType::Connected,
//         payload: None,
//     };

//     let msg: String = serde_json::to_string(&msg).unwrap();
//     session
//         .recipient
//         .unbounded_send(Message::Text(msg))
//         .unwrap();
//     session
// }

// pub async fn destroy<'a>(sessions: &'a mut Sessions, addr: &SocketAddr) {
//     let mut sessions_guard = sessions.lock().await;
//     match sessions_guard.get(addr) {
//         Some(session) => {
//             if session.symbol() != "init" {
//                 log::warn!("Session {} {:?} destroyed!", addr, session.bot_name());
//                 sessions_guard.remove(addr);
//             }
//         }
//         None => {
//             log::error!("Session {} not found.", addr);
//         }
//     };
// }

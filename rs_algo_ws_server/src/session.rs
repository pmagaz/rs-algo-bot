use rs_algo_shared::broker::{Broker, Response, VEC_DOHLC};
use tokio::sync::{mpsc, oneshot};
use rs_algo_shared::broker::xtb::*;

use crate::server::Msg;

#[derive(Debug)]
pub struct Session {
    id: usize,
    subscription: String,
    client_name: String,
    pub broker: Xtb,
    pub tx: mpsc::UnboundedSender<Msg>
}

impl  Session {

    pub async fn new(id: usize, tx: mpsc::UnboundedSender<Msg>, subscription: String, client_name: String)  -> Self {
       Self {
        id,
        subscription,
        client_name,
        tx,
        broker: Xtb::new().await,
       } 
    }
}
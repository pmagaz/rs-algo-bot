use std::time::{Duration, Instant};

use crate::{
    message::{ChatMessage, LeaveRoom, ListRooms, SendMessage, Subscribe},
    server::WsChatServer,
};
use actix::{fut, prelude::*};
use actix_broker::BrokerIssue;
use actix_web_actors::ws;
use rs_algo_shared::broker::{Broker, Response, VEC_DOHLC};
use serde_json::Value;
/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);


pub struct WsChatSession<BK> {
    id: usize,
    subscription: String,
    client_name: String,
    broker: BK,
}

// impl<BK> Default for WsChatSession<BK> where
//     BK: std::marker::Unpin + 'static,
//     BK: Broker, {
//     fn default() -> Self {
//         Self::new().await
//     }
// }

impl<BK> WsChatSession<BK>
where
    BK: std::marker::Unpin + 'static,
    BK: Broker,
{

    pub async fn new()  -> Self {
       Self {
        id: 0,
        subscription: "".to_owned(),
        client_name: "".to_owned(),
        broker: BK::new().await,
       } 
    }
    
    pub fn symbol_subscribe(
        &mut self,
        name: &str,
        client_name: &str,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let name = name.to_owned();

        // First send a leave message for the current subscription
        let leave_msg = LeaveRoom(self.subscription.clone(), self.id);

        // issue_sync comes from having the `BrokerIssue` trait in scope.
        self.issue_system_sync(leave_msg, ctx);

        // Then send a join message for the new subscription
        let join_msg = Subscribe(
            name.to_owned(),
            client_name.to_owned(),
            ctx.address().recipient(),
        );

        WsChatServer::from_registry()
            .send(join_msg)
            .into_actor(self)
            .then(|id, act, _ctx| {
                if let Ok(id) = id {
                    act.id = id;
                    act.subscription = name;
                }

                fut::ready(())
            })
            .wait(ctx);
    }

    pub fn list_rooms(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        WsChatServer::from_registry()
            .send(ListRooms)
            .into_actor(self)
            .then(|res, _, ctx| {
                if let Ok(rooms) = res {
                    for subscription in rooms {
                        ctx.text(subscription);
                    }
                }

                fut::ready(())
            })
            .wait(ctx);
    }

    pub fn send_msg(&self, msg: &str) {
        let content = format!("{}: {msg}", self.client_name.clone());

        println!("11111 {:?}", self.id);

        let msg = SendMessage(self.subscription.clone(), self.id, content);

        // issue_async comes from having the `BrokerIssue` trait in scope.
        self.issue_system_async(msg);
    }

    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            //ctx.ping(b"");
        });
    }
}

impl<BK> Actor for WsChatSession<BK>
where
    BK: std::marker::Unpin + 'static,
    BK: Broker,
{
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("NEW SESSION STARTED");
        self.hb(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!(
            "WsChatSession closed for {}({}) in subscription {}",
            self.client_name.clone(),
            self.id,
            self.subscription
        );
    }
}

impl<BK: std::marker::Unpin + 'static> Handler<ChatMessage> for WsChatSession<BK>
where
    BK: Broker,
{
    type Result = ();

    fn handle(&mut self, msg: ChatMessage, ctx: &mut Self::Context) {
        ctx.text(msg.0);
    }
}

impl<BK> StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsChatSession<BK>
where
    BK: std::marker::Unpin + 'static,
    BK: Broker,
{
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Err(_) => {
                ctx.stop();
                return;
            }
            Ok(msg) => msg,
        };

        log::debug!("WEBSOCKET MESSAGE: {msg:?}");

        match msg {
            ws::Message::Ping(msg) => {
                //self.hb = Instant::now();
                ctx.pong(&msg);
            }
            ws::Message::Pong(_) => {
                log::info!("Pong received");
                //self.hb = Instant::now();
            }
            ws::Message::Text(msg) => {
                let msg: Value = serde_json::from_str(&msg).expect("Can't parse to JSON");
                log::info!("{} command received", &msg["command"]);

                match msg["command"].clone() {
                    Value::String(com) => match com.as_ref() {
                        "subscribe" => {
                            let symbol = match &msg["arguments"]["symbol"] {
                                Value::String(s) => s,
                                _ => panic!("symbol parse error"),
                            };

                            let time_frame = match &msg["arguments"]["time_frame"] {
                                Value::String(s) => s,
                                _ => panic!("time_frame parse error"),
                            };

                            let strategy = match &msg["arguments"]["strategy"] {
                                Value::String(s) => s,
                                _ => panic!("strategy parse error"),
                            };

                            let strategy_type = match &msg["arguments"]["strategy_type"] {
                                Value::String(s) => s,
                                _ => panic!("strategy type parse error"),
                            };

                            let name = &[symbol, "_", time_frame].concat();
                            let client_name = &[strategy, "_", strategy_type].concat();

                            self.symbol_subscribe(name, client_name, ctx);
                        }
                        &_ => log::error!("unknown command received {}", com),
                    },
                    _ => log::error!("wrong command format {:?}", &msg),
                };
            }
            ws::Message::Binary(_) => println!("Unexpected binary"),
            ws::Message::Close(reason) => {
                ctx.close(reason);
                ctx.stop();
            }
            ws::Message::Continuation(_) => {
                ctx.stop();
            }
            ws::Message::Nop => (),
        }
    }
}

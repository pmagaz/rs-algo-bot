use std::time::{Duration, Instant};

use crate::{
    message::{ChatMessage, LeaveRoom, ListRooms, SendMessage, Subscribe},
    server::WsChatServer,
};
use actix::{fut, prelude::*};
use actix_broker::BrokerIssue;
use actix_web_actors::ws;
use serde_json::Value;

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct WsChatSession {
    id: usize,
    subscription: String,
    client_name: String,
}

impl WsChatSession {
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

impl Actor for WsChatSession {
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

impl Handler<ChatMessage> for WsChatSession {
    type Result = ();

    fn handle(&mut self, msg: ChatMessage, ctx: &mut Self::Context) {
        ctx.text(msg.0);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsChatSession {
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

                // we check for /sss type of messages
                // if m.starts_with('/') {
                //     let v: Vec<&str> = m.splitn(2, ' ').collect();
                //     match v[0] {
                //         "/list" => {
                //             // Send ListRooms message to chat server and wait for
                //             // response
                //             println!("List rooms");
                //             self.addr
                //                 .send(server::ListRooms)
                //                 .into_actor(self)
                //                 .then(|res, _, ctx| {
                //                     match res {
                //                         Ok(rooms) => {
                //                             for subscription in rooms {
                //                                 ctx.text(subscription);
                //                             }
                //                         }
                //                         _ => println!("Something is wrong"),
                //                     }
                //                     fut::ready(())
                //                 })
                //                 .wait(ctx)
                //             // .wait(ctx) pauses all events in context,
                //             // so actor wont receive any new messages until it get list
                //             // of rooms back
                //         }
                //         "/join" => {
                //             if v.len() == 2 {
                //                 self.subscription = v[1].to_owned();
                //                 self.addr.do_send(server::Join {
                //                     id: self.id,
                //                     client_name: self.subscription.clone(),
                //                 });

                //                 ctx.text("joined");
                //             } else {
                //                 ctx.text("!!! subscription client_name is required");
                //             }
                //         }
                //         "/client_name" => {
                //             if v.len() == 2 {
                //                 self.client_name = Some(v[1].to_owned());
                //             } else {
                //                 ctx.text("!!! client_name is required");
                //             }
                //         }
                //         _ => ctx.text(format!("!!! unknown command: {m:?}")),
                //     }
                // } else {
                //     let msg = if let Some(ref client_name) = self.client_name {
                //         format!("{client_name}: {m}")
                //     } else {
                //         m.to_owned()
                //     };
                //     // send message to chat server
                //     self.addr.do_send(server::ClientMessage {
                //         id: self.id,
                //         msg,
                //         subscription: self.subscription.clone(),
                //     })
                // }
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

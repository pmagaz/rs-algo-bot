use std::collections::HashMap;

use actix::prelude::*;
use actix_broker::BrokerSubscribe;
use rs_algo_shared::broker::{Broker, Response, VEC_DOHLC};
use rs_algo_shared::broker::xtb::*;
use async_recursion::async_recursion;

use crate::message::{ChatMessage, LeaveRoom, ListRooms, SendMessage, Subscribe};

type Client = Recipient<ChatMessage>;
type subscription = HashMap<usize, Client>;

#[derive(Default)]
pub struct WsChatServer {
    subscriptions: HashMap<String, subscription>,
    brokers: Vec<Box<Xtb>>,
}

impl WsChatServer {
    fn take_room(&mut self, subscription_name: &str) -> Option<subscription> {

        let subscription = self.subscriptions.get_mut(subscription_name)?;
        let subscription = std::mem::take(subscription);
        Some(subscription)
    }

    async fn subscribe_client(
        &mut self,
        subscription_name: &str,
        id: Option<usize>,
        client: Client,
    ) -> usize {
        let mut id = id.unwrap_or_else(rand::random::<usize>);
        
        log::info!("MSG0");
        let mut broker = Xtb::new().await;
        broker.login("11111", "222222").await.unwrap();
        log::info!("MSG1");
        let symbols = broker.get_symbols().await.unwrap().symbols;
        log::info!("MSG2");
        let msg = serde_json::to_string(&symbols).unwrap();

        log::info!("MSG {:?}", msg);
        
        self.send_chat_message(subscription_name,&msg, 1);

        if let Some(subscription) = self.subscriptions.get_mut(subscription_name) {
            loop {
                if subscription.contains_key(&id) {
                    id = rand::random::<usize>();
                } else {
                    break;
                }
            }

            subscription.insert(id, client);
            return id;
        }

        // Create a new subscription for the first client
        let mut subscription: subscription = HashMap::new();

        subscription.insert(id, client);
        self.subscriptions
            .insert(subscription_name.to_owned(), subscription);

        id
    }

    //#[async_recursion]
    fn send_chat_message(&mut self, subscription_name: &str, msg: &str, _src: usize) -> Option<()> {
        let mut subscription = self.take_room(subscription_name)?;

        for (id, client) in subscription.drain() {
            if client.try_send(ChatMessage(msg.to_owned())).is_ok() {
                self.subscribe_client(subscription_name, Some(id), client);
            }
        }

        Some(())
    }
}

impl Actor for WsChatServer {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_system_async::<LeaveRoom>(ctx);
        self.subscribe_system_async::<SendMessage>(ctx);
    }
}

impl Handler<Subscribe> for WsChatServer {
    type Result = MessageResult<Subscribe>;

    fn handle(&mut self, msg: Subscribe, _ctx: &mut Self::Context) -> Self::Result {
        let Subscribe(subscription_name, client_name, client) = msg;

        let id = self.subscribe_client(&subscription_name, None, client);
        let join_msg = format!("{} joined {subscription_name}", client_name,);

        self.send_chat_message(&subscription_name, &join_msg, id);
        MessageResult(id);
        Ok(())
    }
}

impl Handler<LeaveRoom> for WsChatServer {
    type Result = ();

    fn handle(&mut self, msg: LeaveRoom, _ctx: &mut Self::Context) {
        if let Some(subscription) = self.subscriptions.get_mut(&msg.0) {
            subscription.remove(&msg.1);
        }
    }
}

impl Handler<ListRooms> for WsChatServer {
    type Result = MessageResult<ListRooms>;

    fn handle(&mut self, _: ListRooms, _ctx: &mut Self::Context) -> Self::Result {
        MessageResult(self.subscriptions.keys().cloned().collect())
    }
}

impl Handler<SendMessage> for WsChatServer {
    type Result = ();

    fn handle(&mut self, msg: SendMessage, _ctx: &mut Self::Context) {
        let SendMessage(subscription_name, id, msg) = msg;
        self.send_chat_message(&subscription_name, &msg, id);
    }
}

impl SystemService for WsChatServer {}
impl Supervised for WsChatServer {}

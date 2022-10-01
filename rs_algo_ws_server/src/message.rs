use actix::prelude::*;

pub use actix_ws::Message;


#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct ChatMessage(pub String);

#[derive(Clone, Message)]
#[rtype(result = "usize")]
pub struct Subscribe(pub String, pub String, pub Recipient<ChatMessage>);

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct LeaveRoom(pub String, pub usize);

#[derive(Clone, Message)]
#[rtype(result = "Vec<String>")]
pub struct ListRooms;

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct SendMessage(pub String, pub usize, pub String);
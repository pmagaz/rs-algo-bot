//! A multi-room chat server.

use std::{
    collections::{HashMap, HashSet},
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use crate::session::Session;
use rs_algo_shared::broker::xtb::*;
use rs_algo_shared::broker::*;

use rand::{thread_rng, Rng as _};
use tokio::sync::{mpsc, oneshot};
use std::env;

/// Connection ID.
pub type ConnId = usize;

/// Room ID.
pub type RoomId = String;

/// Message sent to a room/client.
pub type Msg = String;


/// A command received by the [`ChatServer`].
#[derive(Debug)]
enum Command {
    Connect {
        conn_tx: mpsc::UnboundedSender<Msg>,
        res_tx: oneshot::Sender<ConnId>,
    },

    Disconnect {
        conn: ConnId,
    },

    List {
        res_tx: oneshot::Sender<Vec<RoomId>>,
    },

    Join {
        conn: ConnId,
        room: RoomId,
        res_tx: oneshot::Sender<()>,
    },

    Message {
        msg: Msg,
        conn: ConnId,
        res_tx: oneshot::Sender<()>,
    },
}


    async fn send_system_message(room: &str, msg: impl Into<String>, rooms: &HashMap<RoomId, HashSet<ConnId>>, leches: &HashMap<ConnId, Session>) {

        log::info!("msssssssg {room}");

        if let Some(sessions) = rooms.get(room) {
            let msg = msg.into();

            for conn_id in sessions {
                //if *conn_id != skip {
                    if let Some(session) = leches.get(conn_id) {
                        // errors if client disconnected abruptly and hasn't been timed-out yet
                        let _ = session.tx.send(msg.clone());
                    }
               // }
            }
        }
    }

/// A multi-room chat server.
///
/// Contains the logic of how connections chat with each other plus room management.
///
/// Call and spawn [`run`](Self::run) to start processing commands.
#[derive(Debug)]
pub struct ChatServer {
    /// Map of connection IDs to their message receivers.
    sessions: HashMap<ConnId, Session>,

    /// Map of room name to participant IDs in that room.
    rooms: HashMap<RoomId, HashSet<ConnId>>,

    /// Tracks total number of historical connections established.
    visitor_count: Arc<AtomicUsize>,

    /// Command receiver.
    cmd_rx: mpsc::UnboundedReceiver<Command>,
}

impl ChatServer {
    pub fn new() -> (Self, ChatServerHandle) {
        // create empty server
        let mut rooms = HashMap::with_capacity(4);

        // create default room
        rooms.insert("main".to_owned(), HashSet::new());

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        (
            Self {
                sessions: HashMap::new(),
                rooms,
                visitor_count: Arc::new(AtomicUsize::new(0)),
                cmd_rx,
            },
            ChatServerHandle { cmd_tx },
        )
    }

    /// Send message to users in a room.
    ///
    /// `skip` is used to prevent messages triggered by a connection also being received by it.
    async fn send_system_message(&self, room: &str, skip: ConnId, msg: impl Into<String>) {

        log::info!("msssssssg {room}");

        if let Some(sessions) = self.rooms.get(room) {
            let msg = msg.into();

            for conn_id in sessions {
                //if *conn_id != skip {
                    if let Some(session) = self.sessions.get(conn_id) {
                        // errors if client disconnected abruptly and hasn't been timed-out yet
                        let _ = session.tx.send(msg.clone());
                    }
               // }
            }
        }
    }

    /// Send message to all other users in current room.
    ///
    /// `conn` is used to find current room and prevent messages sent by a connection also being
    /// received by it.
    async fn send_message(&self, conn: ConnId, msg: String) {
        
        if let Some(room) = self
            .rooms
            .iter()
            .find_map(|(room, participants)| participants.contains(&conn).then_some(room))
        {
            self.send_system_message(room, conn, msg).await;
        };
    }

    /// Register new session and assign unique ID to this session
    async fn connect(&mut self, tx: mpsc::UnboundedSender<Msg>) -> ConnId {
        log::info!("Someone joined");

        // notify all users in same room
        self.send_system_message("main", 0, "Someone joined").await;

        // register session with random connection ID
        let id = thread_rng().gen::<usize>();


        let mut new_session = Session::new(id, tx,"hola".to_string(), "aiods".to_string()).await;
        //new_session.broker.login("11111", "222222").await.unwrap();
        self.sessions.insert(id, new_session);

        // let mut broker = new_session.broker;
        // broker.login("11111", "222222").await.unwrap();
        // log::info!("MSG1");
        // let symbols = broker.get_symbols().await.unwrap().symbols;
        // log::info!("MSG2");
        // let msg = serde_json::to_string(&symbols).unwrap();

        // auto join session to main room
        // self.rooms
        //     .entry("main".to_owned())
        //     .or_insert_with(HashSet::new)
        //     .insert(id);

        let count = self.visitor_count.fetch_add(1, Ordering::SeqCst);
        // self.send_system_message("main", 0, format!("Total visitors {count}"))
        //     .await;

        // send id back
        id
    }

    /// Unregister connection from room map and broadcast disconnection message.
    async fn disconnect(&mut self, conn_id: ConnId) {
        println!("Someone disconnected");

        let mut rooms: Vec<String> = Vec::new();

        // remove sender
        // if self.sessions.remove(&conn_id).is_some() {
        //     // remove session from all rooms
        //     for (name, sessions) in &mut self.rooms {
        //         if sessions.remove(&conn_id) {
        //             rooms.push(name.to_owned());
        //         }
        //     }
        // }

        // send message to other users
        for room in rooms {
            self.send_system_message(&room, 0, "Someone disconnected")
                .await;
        }
    }

    /// Returns list of created room names.
    fn list_rooms(&mut self) -> Vec<String> {
        self.rooms.keys().cloned().collect()
    }

    /// Join room, send disconnect message to old room send join message to new room.
    async fn join_room(&mut self, conn_id: ConnId, room: String) {

        let username = &env::var("BROKER_USERNAME").unwrap();
        let password = &env::var("BROKER_PASSWORD").unwrap();

        let mut rooms = Vec::new();

        log::info!("joining room {conn_id} {room}");

        let session = self.sessions.get(&conn_id).unwrap();
        let mut broker = Xtb::new().await;
        
        log::info!("retrieving session data {conn_id} {room}");
        
        //broker.login(username, password).await.unwrap();
        //let symbols = broker.get_symbols().await.unwrap().symbols;
        //let data = &serde_json::to_string(&symbols).unwrap();
        // remove session from all rooms
        for (n, sessions) in &mut self.rooms {
            if sessions.remove(&conn_id) {
                rooms.push(n.to_owned());
            }
        }
        // send message to other users
        for room in rooms {
            self.send_system_message(&room, 0, "Someone disconnected")
                .await;
        }

        let leches = room.clone();
        let rooms = self.rooms.clone();
        let sessions = &self.sessions;
        //log::info!("11111 {data}");

        broker.listen(|res: Response<VEC_DOHLC>| async move {
            let data = &serde_json::to_string(&res).unwrap();
            log::info!("22222222222");

            //send_system_message(&leches, data, &rooms, &sessions).await;
            Ok(())
        });


        self.rooms
            .entry(room.clone())
            .or_insert_with(HashSet::new)
            .insert(conn_id);


        //  self.send_system_message(&room, conn_id, format!("Subscribing to {room}"))
        //     .await;

        // self.send_system_message(&room, conn_id, data)
        //     .await;
    }

    pub async fn run(mut self) -> io::Result<()> {
        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                Command::Connect { conn_tx, res_tx } => {
                    let conn_id = self.connect(conn_tx).await;
                    let _ = res_tx.send(conn_id);
                }

                Command::Disconnect { conn } => {
                    self.disconnect(conn).await;
                }

                Command::List { res_tx } => {
                    let _ = res_tx.send(self.list_rooms());
                }

                Command::Join { conn, room, res_tx } => {
                    self.join_room(conn, room).await;
                    let _ = res_tx.send(());
                }

                Command::Message { conn, msg, res_tx } => {
                    self.send_message(conn, msg).await;
                    let _ = res_tx.send(());
                }
            }
        }

        Ok(())
    }
}

/// Handle and command sender for chat server.
///
/// Reduces boilerplate of setting up response channels in WebSocket handlers.
#[derive(Debug, Clone)]
pub struct ChatServerHandle {
    cmd_tx: mpsc::UnboundedSender<Command>,
}

impl ChatServerHandle {
    /// Register client message sender and obtain connection ID.
    pub async fn connect(&self, conn_tx: mpsc::UnboundedSender<String>) -> ConnId {
        let (res_tx, res_rx) = oneshot::channel();

        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send(Command::Connect { conn_tx, res_tx })
            .unwrap();

        // unwrap: chat server does not drop out response channel
        res_rx.await.unwrap()
    }

    /// List all created rooms.
    pub async fn list_rooms(&self) -> Vec<String> {
        let (res_tx, res_rx) = oneshot::channel();

        // unwrap: chat server should not have been dropped
        self.cmd_tx.send(Command::List { res_tx }).unwrap();

        // unwrap: chat server does not drop our response channel
        res_rx.await.unwrap()
    }

    /// Join `room`, creating it if it does not exist.
    pub async fn join_room(&self, conn: ConnId, room: String) {
        let (res_tx, res_rx) = oneshot::channel();

        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send(Command::Join {
                conn,
                room: room.into(),
                res_tx,
            })
            .unwrap();

        // unwrap: chat server does not drop our response channel
        res_rx.await.unwrap();
    }

    /// Broadcast message to current room.
    pub async fn send_message(&self, conn: ConnId, msg: String) {
        let (res_tx, res_rx) = oneshot::channel();

        log::info!("MSG: {:?}", msg);


        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send(Command::Message {
                msg: msg.into(),
                conn,
                res_tx,
            })
            .unwrap();

        // unwrap: chat server does not drop our response channel
        res_rx.await.unwrap();
    }

    /// Unregister message sender and broadcast disconnection message to current room.
    pub fn disconnect(&self, conn: ConnId) {
        // unwrap: chat server should not have been dropped
        self.cmd_tx.send(Command::Disconnect { conn }).unwrap();
    }
}
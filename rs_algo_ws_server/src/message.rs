

use crate::server::*;

use serde_json::Value;
use tungstenite::protocol::Message;
use std::net::SocketAddr;


use rs_algo_shared::broker::xtb::*;
use rs_algo_shared::broker::*;


pub fn send_message(sessions: &mut Sessions, addr: &SocketAddr, msg: Message) {
    let recipients = sessions.lock().unwrap();
    match recipients.get(addr) {
        Some(recp) => recp.sender.unbounded_send(msg).unwrap(),
        None => panic!("recipient not found!"),
    };
}

pub fn broadcast_message(sessions: Sessions, addr: SocketAddr, msg: Message) {
    let recipients = sessions.lock().unwrap();
    let broadcast_recipients = recipients
        .iter()
        .filter(|(peer_addr, _)| peer_addr != &&addr)
        .map(|(_, ws_sink)| ws_sink);

    for recp in broadcast_recipients {
        recp.sender.unbounded_send(msg.clone()).unwrap();
    }
}


pub fn handle_message(sessions: &mut Sessions, msg: Message, broker: &mut Xtb) -> Option<String> {
    let data = match msg {
        Message::Ping(bytes) => {
            log::info!("ping received");
            None
            //last_heartbeat = Instant::now();
            // unwrap:
            //session.pong(&bytes).await.unwrap();
        }
        Message::Pong(_) => {
            //let last_heartbeat = Instant::now();
            log::info!("Pong received");
            None
            //last_heartbeat = Instant::now();
        }

        Message::Close(_) => {
            //let last_heartbeat = Instant::now();
            log::info!("Someone Disconected!");
            None
            //last_heartbeat = Instant::now();
        }

        Message::Text(msg) => {
            let msg: Value = serde_json::from_str(&msg).expect("Can't parse to JSON");
            let data = match msg["command"].clone() {
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
                        
                        let res = broker
                            .get_instrument_data2(symbol, 1440, 1656109158)
                            .unwrap();
                        
                            Some(serde_json::to_string(&res).unwrap())
                        //self.symbol_subscribe(name, client_name, ctx);
                    }
                    &_ => {
                        log::error!("unknown command received {}", com);
                        None
                    }
                },
                _ => {
                    log::error!("Wrong command format {:?}", msg);
                    None
                }
            };

            data
        }
        //  Message::Close(msg) => {
        //     println!("Disconected {:?}", msg);
        // }
        _ => {
            println!("Error {:?}", msg);
            None
        }
    };
    data
}
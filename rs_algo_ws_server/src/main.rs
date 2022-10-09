

use dotenv::dotenv;
use std::{env, io::Error as IoError};

mod db;
mod error;
mod heart_beat;
mod message;
mod server;
mod session;

#[tokio::main]
async fn main() -> Result<(), IoError> {
    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let host = env::var("WS_SERVER_HOST").expect("WS_SERVER_HOST not found");
    let port = env::var("WS_SERVER_PORT").expect("WS_SERVER_PORT not found");

    log::info!("WS Server launching on port {port}");
    server::run([host, port].concat()).await;

    Ok(())
}
// //! A simple example of hooking up stdin/stdout to a WebSocket stream.
// //!
// //! This example will connect to a server specified in the argument list and
// //! then forward all data read on stdin to the server, printing out all data
// //! received on stdout.
// //!
// //! Note that this is not currently optimized for performance, especially around
// //! buffer management. Rather it's intended to show an example of working with a
// //! client.
// //!
// //! You can use this example together with the `server` example.

// use core::time;
// use std::{env, thread};

// use futures::SinkExt;
// use futures_util::{future, pin_mut, StreamExt};
// use tokio::io::{AsyncReadExt, AsyncWriteExt};
// use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
// use dotenv::dotenv;
// #[tokio::main]
// async fn main() {
//     dotenv().ok();
//     let url = &env::var("BROKER_STREAM_URL").unwrap();

//     let (stdin_tx, stdin_rx) = futures_channel::mpsc::unbounded();
//     //tokio::spawn(read_stdin(stdin_tx));

//     let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
//     println!("WebSocket handshake has been successfully completed");

//     let (mut write, read) = ws_stream.split();
//     write.send(Message::Ping("hola".as_bytes().to_vec())).await.unwrap();
//     let sleep = time::Duration::from_millis(4300);
//     thread::sleep(sleep);
//     //write.send(Message::Text("hola".to_owned())).await.unwrap();

//     let stdin_to_ws = stdin_rx.map(Ok).forward(write);
//     let ws_to_stdout = {
//         read.for_each(|message| async {
//             let data = message.unwrap().into_data();
//             println!("{:?}", data);
//             //tokio::io::stdout().write_all(&data).await.unwrap();
//         })
//     };

//     pin_mut!(stdin_to_ws, ws_to_stdout);
//     future::select(stdin_to_ws, ws_to_stdout).await;
// }

// // Our helper method which will read data from stdin and send it along the
// // sender provided.
// // async fn read_stdin(tx: futures_channel::mpsc::UnboundedSender<Message>) {
// //     let mut stdin = tokio::io::stdin();
// //     loop {
// //         let mut buf = vec![0; 1024];
// //         let n = match stdin.read(&mut buf).await {
// //             Err(_) | Ok(0) => break,
// //             Ok(n) => n,
// //         };
// //         buf.truncate(n);
// //         tx.unbounded_send(Message::binary(buf)).unwrap();
// //     }
// // }
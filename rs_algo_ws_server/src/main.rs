use dotenv::dotenv;
use std::{env, io::Error as IoError};

mod db;
mod error;
mod handlers;
mod heart_beat;
mod message;
mod server;

#[tokio::main]
async fn main() -> Result<(), IoError> {
    dotenv().ok();

    rs_algo_shared::trace::initialize()
        .unwrap_or_else(|e| eprintln!("Failed to initialize logging: {}", e));

    let host = env::var("WS_SERVER_HOST").expect("WS_SERVER_HOST not found");
    let port = env::var("WS_SERVER_PORT").expect("WS_SERVER_PORT not found");

    tracing::info!("WS Server launching on port {port}");
    server::run([host, port].concat()).await.unwrap();

    Ok(())
}

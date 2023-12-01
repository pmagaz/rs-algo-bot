use dotenv::dotenv;
use std::{env, io::Error as IoError};

mod db;
mod error;
mod handlers;
mod heart_beat;
mod helpers;
mod message;
mod server;

#[tokio::main]
async fn main() -> Result<(), IoError> {
    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let host = env::var("WS_SERVER_HOST").expect("WS_SERVER_HOST not found");
    let port = env::var("WS_SERVER_PORT").expect("WS_SERVER_PORT not found");

    log::info!("WS Server launching on port {port}");
    server::run([host, port].concat()).await.unwrap();

    Ok(())
}

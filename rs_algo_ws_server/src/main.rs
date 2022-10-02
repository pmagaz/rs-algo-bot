
use std::{env, io::Error as IoError};
use dotenv::dotenv;

mod message;
mod server;
mod hb;

#[tokio::main]
async fn main() -> Result<(), IoError> {

    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    
    let host = "0.0.0.0:";
    let port = env::var("WS_SERVER_PORT").expect("WS_SERVER_PORT not found");

    log::info!("WS Server launching on port {port}");
    server::run([host, &port].concat()).await;

    Ok(())
}

pub use rs_algo_shared::error::RsAlgoError;
use rs_algo_shared::ws::message::CommandType;

use thiserror::Error;

pub type Result<T> = ::anyhow::Result<T, RsAlgoError>;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Error)]
pub enum RsAlgoErrorKind {
    #[error("EnvVarNotFound!")]
    EnvVarNotFound,
    #[error("SocketError!")]
    SocketError,
    #[error("InvalidAddress!")]
    InvalidAddress,
    #[error("No Db Connection!")]
    NoDbConnection,
    #[error("Invalid Instrument!")]
    WrongInstrumentConf,
    #[error("Invalid Peak!")]
    InvalidPeak,
    #[error("Error on Request!")]
    RequestError,
}

pub fn serialization(err: serde_json::Error, command: &CommandType) -> String {
    tracing::error!("Serialization failed for {:?}: {}", command, err);
    String::new()
}

pub fn executed_command(
    err: rs_algo_shared::error::RsAlgoError,
    command: &CommandType,
) -> Option<String> {
    tracing::error!("Command {:?} failed: {}", command, err);
    None
}

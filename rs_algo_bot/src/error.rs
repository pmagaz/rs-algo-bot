pub use rs_algo_shared::error::RsAlgoError;
pub use rs_algo_shared::error::RsAlgoErrorKind;

pub type Result<T> = ::anyhow::Result<T, RsAlgoError>;

// #[derive(Copy, Clone, Eq, PartialEq, Debug, Error)]
// pub enum RsAlgoErrorKind {
//     #[error("No Db Connection!")]
//     NoDbConnection,
//     #[error("Invalid Instrument!")]
//     WrongInstrumentConf,
//     #[error("Invalid Peak!")]
//     InvalidPeak,
//     #[error("Error on Request!")]
//     RequestError,
// }

// #[derive(Debug, Error)]
// pub struct RsAlgoError {
//     pub err: RsAlgoErrorKind,
// }

// impl RsAlgoError {
//     pub fn kind(&self) -> RsAlgoErrorKind {
//         self.err
//     }
// }

// impl From<RsAlgoErrorKind> for RsAlgoError {
//     fn from(kind: RsAlgoErrorKind) -> RsAlgoError {
//         RsAlgoError { err: kind }
//     }
// }

// impl Display for RsAlgoError {
//     fn fmt(&self, err: &mut fmt::Formatter<'_>) -> fmt::Result {
//         Display::fmt(&self.err, err)
//     }
// }

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid time component range: {0}")]
    Time(#[from] time::error::ComponentRange),
    #[error("Date calculation error: {0}")]
    DateCalculation(String),
    #[error("Serde JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, Error>;

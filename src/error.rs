use thiserror::Error;

#[derive(Debug, Error)]
pub enum SdkError {
    #[error("http error: {0}")]
    Http(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("chain error: {0}")]
    Chain(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

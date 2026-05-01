use thiserror::Error;

#[derive(Error, Debug)]
pub enum McpError {
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("adapter error: {0}")]
    Adapter(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

pub type Result<T> = std::result::Result<T, McpError>;

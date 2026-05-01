use thiserror::Error;

#[derive(Error, Debug)]
pub enum GatewayError {
    #[error("gRPC error: {0}")]
    Grpc(String),
    #[error("REST error: {0}")]
    Rest(String),
    #[error("WebSocket error: {0}")]
    WebSocket(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error(transparent)]
    Core(#[from] adp_core::AdpError),
}

pub type Result<T> = std::result::Result<T, GatewayError>;

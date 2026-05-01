use thiserror::Error;

#[derive(Error, Debug)]
pub enum RouterError {
    #[error("model loading failed: {0}")]
    ModelLoad(String),
    #[error("inference failed: {0}")]
    Inference(String),
    #[error("token budget exceeded: {0}")]
    TokenBudgetExceeded(String),
    #[error("model not found: {0}")]
    ModelNotFound(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

pub type Result<T> = std::result::Result<T, RouterError>;

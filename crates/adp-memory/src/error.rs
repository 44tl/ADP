use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("vector store error: {0}")]
    VectorStore(String),
    #[error("conversation error: {0}")]
    Conversation(String),
    #[error("context window exceeded: {0}")]
    ContextWindowExceeded(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

pub type Result<T> = std::result::Result<T, MemoryError>;

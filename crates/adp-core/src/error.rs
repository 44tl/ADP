//! Error types for ADP Core.
//!
//! Uses [`thiserror`] for structured, typed errors. Binaries should use [`anyhow`]
//! at the application boundary.

use crate::task::TaskState;
use thiserror::Error;

/// The primary error type for ADP Core operations.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum AdpError {
    /// Attempted an invalid state machine transition.
    #[error("invalid state transition: {from} -> {to}")]
    InvalidStateTransition { from: TaskState, to: TaskState },

    /// Task ID not found in the store.
    #[error("task not found: {0}")]
    TaskNotFound(String),

    /// Attempted to transition a task already in a terminal state.
    #[error("task already in terminal state: {0}")]
    TerminalState(TaskState),

    /// Delegation or execution failed.
    #[error("delegation failed: {0}")]
    DelegationFailed(String),

    /// Underlying persistence error.
    #[error("store error: {0}")]
    StoreError(String),

    /// JSON serialization/deserialization failed.
    #[error("serialization error: {0}")]
    SerializationError(String),
}

/// Shorthand result type used throughout ADP Core.
pub type Result<T> = std::result::Result<T, AdpError>;

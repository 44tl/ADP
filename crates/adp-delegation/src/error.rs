//! Error types for ADP Delegation.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DelegationError {
    /// No agent found matching the required capabilities.
    #[error("no matching agent: {0}")]
    NoMatchingAgent(String),

    /// Strategy execution failed.
    #[error("strategy failed: {0}")]
    StrategyFailed(String),

    /// Consensus could not be reached.
    #[error("consensus failed: {0}")]
    ConsensusFailed(String),

    /// Underlying core error.
    #[error(transparent)]
    Core(#[from] adp_core::AdpError),

    /// Underlying runtime error.
    #[error(transparent)]
    Runtime(#[from] adp_runtime::RuntimeError),
}

pub type Result<T> = std::result::Result<T, DelegationError>;

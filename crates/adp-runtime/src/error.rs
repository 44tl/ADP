//! Error types for ADP Runtime.

use thiserror::Error;

/// The primary error type for ADP Runtime operations.
#[derive(Error, Debug)]
pub enum RuntimeError {
    /// WASM instantiation or execution failed.
    #[error("wasm error: {0}")]
    Wasm(String),

    /// Agent violated a capability boundary.
    #[error("capability denied: {capability} for agent {agent_id}")]
    CapabilityDenied { agent_id: String, capability: String },

    /// Resource limit exceeded (fuel, memory, or timeout).
    #[error("resource limit exceeded: {limit}")]
    ResourceLimitExceeded { limit: String },

    /// Agent process or task failed.
    #[error("agent execution failed: {0}")]
    AgentExecutionFailed(String),

    /// MCP server communication error.
    #[error("mcp error: {0}")]
    Mcp(String),

    /// Underlying core error.
    #[error(transparent)]
    Core(#[from] adp_core::AdpError),

    /// Serialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),
}

pub type Result<T> = std::result::Result<T, RuntimeError>;

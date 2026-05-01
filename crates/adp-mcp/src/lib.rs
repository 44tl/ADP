//! ADP MCP — Model Context Protocol definitions and adapters.
//!
//! This crate provides the protocol layer for MCP, separate from the
//! runtime client in [`adp-runtime`]. It contains:
//!
//! - [`protocol`] — MCP message types and JSON-RPC framing.
//! - [`adapter`] — Adapters for converting between MCP tools and ADP capabilities.

pub mod adapter;
pub mod error;
pub mod protocol;

pub use adapter::McpAdapter;
pub use error::{McpError, Result};
pub use protocol::{McpMessage, McpRequest, McpResponse, McpTool};

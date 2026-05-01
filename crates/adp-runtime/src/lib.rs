//! ADP Runtime — Agent lifecycle, WASM sandbox, and resource limits.
//!
//! This crate sits above [`adp-core`] and is responsible for:
//! - Managing agent processes and their lifecycle.
//! - Executing agent code inside a [`wasmtime`] sandbox with strict resource limits.
//! - Enforcing capability-based security: agents cannot perform I/O unless explicitly granted.
//! - Communicating with MCP (Model Context Protocol) servers via stdio.
//!
//! # Security Model
//!
//! 1. **No ambient authority**: By default, agents have no filesystem, network, or environment access.
//! 2. **Capability grants**: The host runtime explicitly delegates capabilities to agents at spawn time.
//! 3. **Resource caps**: Every agent execution is bounded by fuel (WASM instructions), memory, and wall-clock time.
//! 4. **Host functions**: All agent I/O goes through audited host functions that check capabilities.

pub mod agent;
pub mod capabilities;
pub mod error;
pub mod mcp;
pub mod resources;
pub mod sandbox;

pub use agent::{AgentHandle, AgentLifecycle, AgentManifest};
pub use capabilities::{Capability, CapabilitySet};
pub use error::{RuntimeError, Result};
pub use resources::ResourceLimits;
pub use sandbox::{Sandbox, SandboxConfig, SandboxOutcome};

//! Capability-based security for agent execution.
//!
//! Agents start with **zero** capabilities. The host runtime must explicitly
//! grant capabilities via [`CapabilitySet`] before spawning an agent.
//!
//! # Example
//!
//! ```
//! use adp_runtime::capabilities::{Capability, CapabilitySet};
//!
//! let caps = CapabilitySet::new()
//!     .grant(Capability::FileRead { path: "/data".into() })
//!     .grant(Capability::HttpRequest { allowlist: vec!["api.local".into()] });
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A single capability granted to an agent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "config")]
pub enum Capability {
    /// Read access to a specific directory or file.
    FileRead { path: String },
    /// Write access to a specific directory or file.
    FileWrite { path: String },
    /// HTTP requests allowed to specific hosts.
    HttpRequest { allowlist: Vec<String> },
    /// Execute external tools or MCP servers.
    ToolExecution { tools: Vec<String> },
    /// Access to the vector database (Qdrant) for memory/RAG.
    VectorDbAccess { collections: Vec<String> },
    /// Access to the event log (read-only).
    EventLogRead,
    /// Access to spawn child agents (delegation).
    SpawnAgent,
}

impl Capability {
    /// Human-readable name for this capability variant.
    pub fn name(&self) -> &'static str {
        match self {
            Capability::FileRead { .. } => "file:read",
            Capability::FileWrite { .. } => "file:write",
            Capability::HttpRequest { .. } => "http:request",
            Capability::ToolExecution { .. } => "tool:execute",
            Capability::VectorDbAccess { .. } => "vectordb:access",
            Capability::EventLogRead => "eventlog:read",
            Capability::SpawnAgent => "agent:spawn",
        }
    }
}

/// A set of capabilities granted to an agent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySet {
    capabilities: HashSet<Capability>,
}

impl CapabilitySet {
    /// Create an empty capability set (no permissions).
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant a capability.
    pub fn grant(mut self, cap: Capability) -> Self {
        self.capabilities.insert(cap);
        self
    }

    /// Revoke a capability.
    pub fn revoke(&mut self, cap: &Capability) {
        self.capabilities.remove(cap);
    }

    /// Check whether a specific capability is granted.
    pub fn has(&self, cap: &Capability) -> bool {
        self.capabilities.contains(cap)
    }

    /// Check whether any capability of a given category is granted.
    pub fn has_category(&self, category: &str) -> bool {
        self.capabilities.iter().any(|c| c.name().starts_with(category))
    }

    /// Iterate over granted capabilities.
    pub fn iter(&self) -> impl Iterator<Item = &Capability> {
        self.capabilities.iter()
    }

    /// Returns `true` if no capabilities are granted.
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }
}

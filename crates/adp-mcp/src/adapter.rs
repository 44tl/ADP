//! MCP-to-ADP capability adapter.
//!
//! Converts MCP tool definitions into ADP [`Capability`](adp_runtime::capabilities::Capability)
//! grants, and vice versa.

use crate::error::{McpError, Result};
use crate::protocol::McpTool;
use adp_runtime::capabilities::{Capability, CapabilitySet};

/// Adapter between MCP tools and ADP capabilities.
#[derive(Debug, Clone, Default)]
pub struct McpAdapter;

impl McpAdapter {
    /// Convert an MCP tool to an ADP capability.
    pub fn tool_to_capability(tool: &McpTool) -> Result<Capability> {
        match tool.name.as_str() {
            name if name.starts_with("file_read") => Ok(Capability::FileRead {
                path: "/".to_string(), // Conservative default
            }),
            name if name.starts_with("file_write") => Ok(Capability::FileWrite {
                path: "/".to_string(),
            }),
            name if name.starts_with("http") => Ok(Capability::HttpRequest {
                allowlist: Vec::new(),
            }),
            name if name.starts_with("browser") => Ok(Capability::ToolExecution {
                tools: vec!["browser".to_string()],
            }),
            _ => Ok(Capability::ToolExecution {
                tools: vec![tool.name.clone()],
            }),
        }
    }

    /// Convert a list of MCP tools to a capability set.
    pub fn tools_to_capabilities(tools: &[McpTool]) -> Result<CapabilitySet> {
        let mut caps = CapabilitySet::new();
        for tool in tools {
            caps = caps.grant(Self::tool_to_capability(tool)?);
        }
        Ok(caps)
    }

    /// Convert an ADP capability back to an MCP tool name (best-effort).
    pub fn capability_to_tool_name(cap: &Capability) -> String {
        match cap {
            Capability::FileRead { .. } => "file_read".to_string(),
            Capability::FileWrite { .. } => "file_write".to_string(),
            Capability::HttpRequest { .. } => "http_request".to_string(),
            Capability::ToolExecution { tools } => tools.first().cloned().unwrap_or_default(),
            Capability::VectorDbAccess { .. } => "vector_db".to_string(),
            Capability::EventLogRead => "event_log_read".to_string(),
            Capability::SpawnAgent => "spawn_agent".to_string(),
        }
    }
}

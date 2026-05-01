//! Agent registry — discovery and capability-based matching.
//!
//! The [`AgentRegistry`] maintains a catalog of available agents and their
//! capabilities. It is the source of truth for the delegation engine when
//! deciding which agent(s) can handle a given task.

use adp_core::task::Id;
use adp_runtime::capabilities::CapabilitySet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, instrument};

/// A registered agent entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub agent_id: Id,
    pub name: String,
    pub capabilities: CapabilitySet,
    pub metadata: serde_json::Value,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
}

/// In-memory agent registry with heartbeat-based liveness.
#[derive(Debug, Clone, Default)]
pub struct AgentRegistry {
    agents: Arc<RwLock<HashMap<Id, RegistryEntry>>>,
}

impl AgentRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new agent or update an existing one.
    #[instrument(skip(self, entry), fields(agent_id = %entry.agent_id))]
    pub async fn register(&self, entry: RegistryEntry) {
        let mut agents = self.agents.write().await;
        debug!(agent_name = %entry.name, "agent registered");
        agents.insert(entry.agent_id, entry);
    }

    /// Remove an agent from the registry.
    #[instrument(skip(self), fields(agent_id = %agent_id))]
    pub async fn unregister(&self, agent_id: Id) {
        let mut agents = self.agents.write().await;
        agents.remove(&agent_id);
        debug!("agent unregistered");
    }

    /// Update the heartbeat timestamp for an agent.
    #[instrument(skip(self), fields(agent_id = %agent_id))]
    pub async fn heartbeat(&self, agent_id: Id) {
        let mut agents = self.agents.write().await;
        if let Some(entry) = agents.get_mut(&agent_id) {
            entry.last_heartbeat = chrono::Utc::now();
            debug!("heartbeat updated");
        }
    }

    /// Find all agents that have **all** the required capabilities.
    #[instrument(skip(self))]
    pub async fn find_matching(&self, required: &CapabilitySet) -> Vec<RegistryEntry> {
        let agents = self.agents.read().await;
        agents
            .values()
            .filter(|entry| {
                required.iter().all(|cap| entry.capabilities.has(cap))
            })
            .cloned()
            .collect()
    }

    /// Find a single agent by exact capability match (best for SINGLE strategy).
    #[instrument(skip(self))]
    pub async fn find_one(&self, required: &CapabilitySet) -> Option<RegistryEntry> {
        let agents = self.agents.read().await;
        agents
            .values()
            .find(|entry| {
                required.iter().all(|cap| entry.capabilities.has(cap))
            })
            .cloned()
    }

    /// Get all registered agents.
    pub async fn list_all(&self) -> Vec<RegistryEntry> {
        let agents = self.agents.read().await;
        agents.values().cloned().collect()
    }

    /// Get a specific agent by ID.
    pub async fn get(&self, agent_id: &Id) -> Option<RegistryEntry> {
        let agents = self.agents.read().await;
        agents.get(agent_id).cloned()
    }

    /// Remove agents that haven't sent a heartbeat within the given duration.
    #[instrument(skip(self))]
    pub async fn prune_stale(&self, max_age: std::time::Duration) {
        let cutoff = chrono::Utc::now() - chrono::Duration::from_std(max_age).unwrap_or_default();
        let mut agents = self.agents.write().await;
        let before = agents.len();
        agents.retain(|_, entry| entry.last_heartbeat > cutoff);
        let after = agents.len();
        if before != after {
            debug!(pruned = before - after, "stale agents removed");
        }
    }
}

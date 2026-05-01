//! Agent lifecycle management.
//!
//! An [`AgentManifest`] describes an agent's code, capabilities, and resource limits.
//! The [`AgentLifecycle`] service spawns agents into a [`Sandbox`](crate::sandbox::Sandbox),
//! returning an [`AgentHandle`] that can be used to monitor and control execution.

use crate::capabilities::CapabilitySet;
use crate::error::{RuntimeError, Result};
use crate::resources::ResourceLimits;
use crate::sandbox::{Sandbox, SandboxConfig, SandboxOutcome};
use adp_core::task::Id;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, instrument, warn};

/// Static description of an agent.
#[derive(Debug, Clone)]
pub struct AgentManifest {
    /// Unique agent identifier.
    pub id: Id,
    /// Human-readable name.
    pub name: String,
    /// Path to the compiled WASM module.
    pub wasm_path: PathBuf,
    /// Capabilities granted to this agent.
    pub capabilities: CapabilitySet,
    /// Resource limits for this agent.
    pub resource_limits: ResourceLimits,
    /// Entrypoint function name (default: `_start`).
    pub entrypoint: String,
    /// Environment variables injected into the sandbox.
    pub env: Vec<(String, String)>,
}

impl AgentManifest {
    /// Start building a manifest.
    pub fn builder(name: impl Into<String>, wasm_path: impl Into<PathBuf>) -> AgentManifestBuilder {
        AgentManifestBuilder::new(name, wasm_path)
    }
}

/// Builder for [`AgentManifest`].
#[derive(Debug)]
pub struct AgentManifestBuilder {
    name: String,
    wasm_path: PathBuf,
    capabilities: CapabilitySet,
    resource_limits: ResourceLimits,
    entrypoint: String,
    env: Vec<(String, String)>,
}

impl AgentManifestBuilder {
    fn new(name: impl Into<String>, wasm_path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            wasm_path: wasm_path.into(),
            capabilities: CapabilitySet::new(),
            resource_limits: ResourceLimits::default(),
            entrypoint: "_start".to_string(),
            env: Vec::new(),
        }
    }

    pub fn capabilities(mut self, caps: CapabilitySet) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn resource_limits(mut self, limits: ResourceLimits) -> Self {
        self.resource_limits = limits;
        self
    }

    pub fn entrypoint(mut self, entry: impl Into<String>) -> Self {
        self.entrypoint = entry.into();
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn build(self) -> AgentManifest {
        AgentManifest {
            id: Id::new(),
            name: self.name,
            wasm_path: self.wasm_path,
            capabilities: self.capabilities,
            resource_limits: self.resource_limits,
            entrypoint: self.entrypoint,
            env: self.env,
        }
    }
}

/// Handle to a running (or completed) agent instance.
#[derive(Debug)]
pub struct AgentHandle {
    pub id: Id,
    pub manifest: AgentManifest,
    /// Channel to receive execution outcome.
    pub outcome_rx: oneshot::Receiver<SandboxOutcome>,
    /// Channel to send control commands (e.g., cancel).
    pub control_tx: mpsc::Sender<AgentControlCommand>,
}

/// Commands that can be sent to a running agent.
#[derive(Debug, Clone)]
pub enum AgentControlCommand {
    /// Request graceful cancellation.
    Cancel,
    /// Update resource limits mid-flight (best-effort).
    UpdateLimits(ResourceLimits),
}

/// Service responsible for spawning and managing agent lifecycles.
#[derive(Debug, Clone)]
pub struct AgentLifecycle {
    sandbox: Arc<Sandbox>,
}

impl AgentLifecycle {
    /// Create a new lifecycle manager with the given sandbox configuration.
    pub fn new(sandbox_config: SandboxConfig) -> Result<Self> {
        let sandbox = Sandbox::new(sandbox_config)?;
        Ok(Self {
            sandbox: Arc::new(sandbox),
        })
    }

    /// Spawn an agent according to its manifest.
    ///
    /// Returns immediately with an [`AgentHandle`]. The actual execution runs
    /// on a dedicated Tokio task.
    #[instrument(skip(self, manifest), fields(agent_id = %manifest.id, agent_name = %manifest.name))]
    pub fn spawn(&self, manifest: AgentManifest) -> Result<AgentHandle> {
        let id = manifest.id;
        let sandbox = Arc::clone(&self.sandbox);
        let manifest_clone = manifest.clone();

        let (outcome_tx, outcome_rx) = oneshot::channel();
        let (control_tx, mut control_rx) = mpsc::channel::<AgentControlCommand>(4);

        tokio::spawn(async move {
            info!(agent_id = %id, "agent execution started");

            let config = SandboxConfig {
                wasm_path: manifest_clone.wasm_path.clone(),
                entrypoint: manifest_clone.entrypoint.clone(),
                resource_limits: manifest_clone.resource_limits,
                capabilities: manifest_clone.capabilities.clone(),
                env: manifest_clone.env.clone(),
            };

            // Run the sandbox with timeout and cancellation support.
            let outcome = tokio::select! {
                biased;

                // Control command: cancellation
                Some(cmd) = control_rx.recv() => {
                    match cmd {
                        AgentControlCommand::Cancel => {
                            warn!(agent_id = %id, "agent cancelled by control command");
                            SandboxOutcome::Cancelled {
                                reason: "control command".to_string(),
                            }
                        }
                        AgentControlCommand::UpdateLimits(new_limits) => {
                            debug!(agent_id = %id, "updating resource limits mid-flight");
                            // For now, just re-run with new limits. In production,
                            // this would adjust the live store.
                            sandbox.run(config.with_limits(new_limits)).await
                        }
                    }
                }

                // Normal execution
                result = sandbox.run(config) => result,
            };

            info!(agent_id = %id, outcome = ?outcome, "agent execution finished");
            let _ = outcome_tx.send(outcome);
        });

        Ok(AgentHandle {
            id,
            manifest,
            outcome_rx,
            control_tx,
        })
    }
}

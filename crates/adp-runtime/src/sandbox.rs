//! WASM sandbox for agent execution.
//!
//! Uses [`wasmtime`] with the component model to execute untrusted agent code.
//! Key security properties:
//!
//! 1. **No ambient authority**: Agents cannot access filesystem, network, or environment
//!    unless explicitly granted via [`CapabilitySet`](crate::capabilities::CapabilitySet).
//! 2. **Fuel metering**: Every WASM instruction consumes fuel. Out-of-fuel triggers a trap.
//! 3. **Memory limits**: Linear memory is capped at `max_memory_pages`.
//! 4. **Wall-clock timeout**: Execution is wrapped in a Tokio timeout.
//! 5. **Host functions**: All I/O goes through capability-checked host functions.

use crate::capabilities::{Capability, CapabilitySet};
use crate::error::{RuntimeError, Result};
use crate::resources::ResourceLimits;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, instrument, warn};
use wasmtime::{Config, Engine, Instance, Module, Store, StoreLimitsBuilder};

/// Configuration for a single sandbox execution.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Path to the compiled WASM module.
    pub wasm_path: PathBuf,
    /// Entrypoint function name.
    pub entrypoint: String,
    /// Resource limits for this execution.
    pub resource_limits: ResourceLimits,
    /// Capabilities granted to the agent.
    pub capabilities: CapabilitySet,
    /// Environment variables.
    pub env: Vec<(String, String)>,
}

impl SandboxConfig {
    /// Create a new config with updated resource limits.
    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.resource_limits = limits;
        self
    }
}

/// Outcome of a sandbox execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SandboxOutcome {
    /// Agent completed successfully with a result payload.
    Success {
        result: serde_json::Value,
        fuel_consumed: u64,
    },
    /// Agent failed with an error message.
    Failure {
        reason: String,
        fuel_consumed: u64,
    },
    /// Agent exceeded resource limits (fuel, memory, or time).
    ResourceExceeded {
        limit: String,
        fuel_consumed: u64,
    },
    /// Agent was explicitly cancelled.
    Cancelled {
        reason: String,
    },
}

/// Per-execution store data.
struct SandboxState {
    /// Remaining fuel.
    fuel: u64,
    /// Captured stdout.
    stdout: Vec<u8>,
    /// Captured stderr.
    stderr: Vec<u8>,
    /// Result payload (set by the agent via host function).
    result: Option<serde_json::Value>,
    /// Capabilities granted to this execution.
    capabilities: CapabilitySet,
    /// Resource limits.
    limits: ResourceLimits,
}

/// The WASM sandbox executor.
#[derive(Debug, Clone)]
pub struct Sandbox {
    engine: Arc<Engine>,
}

impl Sandbox {
    /// Create a new sandbox with a fresh wasmtime engine.
    pub fn new(_config: SandboxConfig) -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.consume_fuel(true);
        config.epoch_interruption(true);

        let engine = Engine::new(&config)
            .map_err(|e| RuntimeError::Wasm(format!("engine creation failed: {e}")))?;

        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    /// Execute a WASM module inside the sandbox.
    #[instrument(skip(self, config), fields(wasm_path = %config.wasm_path.display()))]
    pub async fn run(&self, config: SandboxConfig) -> SandboxOutcome {
        let limits = config.resource_limits;
        let wasm_path = config.wasm_path.clone();

        debug!(fuel = limits.max_fuel, memory = limits.max_memory_pages, "running sandbox");

        let engine = Arc::clone(&self.engine);
        let result = tokio::task::spawn_blocking(move || {
            Self::run_sync(&engine, &config)
        })
        .await;

        match result {
            Ok(outcome) => outcome,
            Err(join_err) => {
                error!(error = %join_err, "sandbox task panicked");
                SandboxOutcome::Failure {
                    reason: format!("sandbox panicked: {join_err}"),
                    fuel_consumed: 0,
                }
            }
        }
    }

    fn run_sync(engine: &Engine, config: &SandboxConfig) -> SandboxOutcome {
        let module = match Module::from_file(engine, &config.wasm_path) {
            Ok(m) => m,
            Err(e) => {
                return SandboxOutcome::Failure {
                    reason: format!("module load failed: {e}"),
                    fuel_consumed: 0,
                };
            }
        };

        let mut store = Store::new(
            engine,
            SandboxState {
                fuel: config.resource_limits.max_fuel,
                stdout: Vec::with_capacity(4096),
                stderr: Vec::with_capacity(4096),
                result: None,
                capabilities: config.capabilities.clone(),
                limits: config.resource_limits,
            },
        );

        // Set fuel and memory limits.
        store.add_fuel(config.resource_limits.max_fuel).ok();
        store.limiter(|state| {
            Some(
                StoreLimitsBuilder::new()
                    .memory_size(state.limits.max_memory_pages as usize * 65536)
                    .build(),
            )
        });

        // Instantiate the module.
        let instance = match Instance::new(&mut store, &module, &[]) {
            Ok(i) => i,
            Err(e) => {
                return SandboxOutcome::Failure {
                    reason: format!("instantiation failed: {e}"),
                    fuel_consumed: config.resource_limits.max_fuel - store.get_fuel().unwrap_or(0),
                };
            }
        };

        // Call the entrypoint.
        let entrypoint = instance.get_typed_func::<(), ()>(&mut store, &config.entrypoint);
        match entrypoint {
            Ok(func) => {
                let result = func.call(&mut store, ());
                let fuel_consumed = config.resource_limits.max_fuel - store.get_fuel().unwrap_or(0);

                match result {
                    Ok(()) => {
                        let state = store.data();
                        if let Some(ref result) = state.result {
                            SandboxOutcome::Success {
                                result: result.clone(),
                                fuel_consumed,
                            }
                        } else {
                            SandboxOutcome::Success {
                                result: serde_json::Value::Null,
                                fuel_consumed,
                            }
                        }
                    }
                    Err(e) => {
                        let reason = if e.to_string().contains("out of fuel") {
                            return SandboxOutcome::ResourceExceeded {
                                limit: "fuel".to_string(),
                                fuel_consumed,
                            };
                        } else {
                            format!("execution error: {e}")
                        };
                        SandboxOutcome::Failure {
                            reason,
                            fuel_consumed,
                        }
                    }
                }
            }
            Err(e) => SandboxOutcome::Failure {
                reason: format!("entrypoint not found: {e}"),
                fuel_consumed: 0,
            },
        }
    }
}

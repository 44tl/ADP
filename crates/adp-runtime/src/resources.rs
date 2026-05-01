//! Resource limits for sandboxed agent execution.
//!
//! Every agent execution is bounded by:
//! - **Fuel**: WASM instruction count (deterministic, cross-platform).
//! - **Memory**: Maximum linear memory pages (64 KiB each).
//! - **Wall-clock time**: Maximum execution duration.
//!
//! Defaults are conservative. Host operators may override per-agent.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Resource limits enforced by the [`Sandbox`](crate::sandbox::Sandbox).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum WASM fuel (instructions). Default: 10 billion.
    pub max_fuel: u64,
    /// Maximum linear memory pages (64 KiB each). Default: 512 (~32 MiB).
    pub max_memory_pages: u64,
    /// Maximum execution time. Default: 5 minutes.
    #[serde(with = "humantime_serde")]
    pub max_execution_time: Duration,
    /// Maximum size of stdout/stderr capture (bytes). Default: 1 MiB.
    pub max_output_size: usize,
}

impl ResourceLimits {
    /// Create a new builder.
    pub fn builder() -> ResourceLimitsBuilder {
        ResourceLimitsBuilder::new()
    }

    /// Conservative defaults suitable for untrusted agents.
    pub const fn default() -> Self {
        Self {
            max_fuel: 10_000_000_000,
            max_memory_pages: 512,
            max_execution_time: Duration::from_secs(300),
            max_output_size: 1_048_576,
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self::default()
    }
}

/// Builder for [`ResourceLimits`].
#[derive(Debug)]
pub struct ResourceLimitsBuilder {
    limits: ResourceLimits,
}

impl ResourceLimitsBuilder {
    fn new() -> Self {
        Self {
            limits: ResourceLimits::default(),
        }
    }

    pub fn max_fuel(mut self, fuel: u64) -> Self {
        self.limits.max_fuel = fuel;
        self
    }

    pub fn max_memory_pages(mut self, pages: u64) -> Self {
        self.limits.max_memory_pages = pages;
        self
    }

    pub fn max_execution_time(mut self, duration: Duration) -> Self {
        self.limits.max_execution_time = duration;
        self
    }

    pub fn max_output_size(mut self, size: usize) -> Self {
        self.limits.max_output_size = size;
        self
    }

    pub fn build(self) -> ResourceLimits {
        self.limits
    }
}

//! Architect Agent — Receives requirements. Outputs ADR + module structure.
//!
//! # Input Context
//!
//! ```json
//! {
//!   "requirements": "Build a task queue with priority scheduling",
//!   "constraints": ["local-first", "no cloud dependencies"]
//! }
//! ```
//!
//! # Output
//!
//! ```json
//! {
//!   "adr": {
//!     "title": "ADR-0042: Priority Task Queue",
//!     "status": "proposed",
//!     "context": "...",
//!     "decision": "...",
//!     "consequences": "..."
//!   },
//!   "modules": [
//!     { "name": "queue", "responsibilities": ["..."] },
//!     { "name": "scheduler", "responsibilities": ["..."] }
//!   ]
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct ArchitectInput {
    pub requirements: String,
    #[serde(default)]
    pub constraints: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Adr {
    pub title: String,
    pub status: String,
    pub context: String,
    pub decision: String,
    pub consequences: String,
}

#[derive(Debug, Serialize)]
pub struct Module {
    pub name: String,
    pub responsibilities: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ArchitectOutput {
    pub adr: Adr,
    pub modules: Vec<Module>,
}

#[no_mangle]
pub extern "C" fn _start() {
    // Placeholder
}

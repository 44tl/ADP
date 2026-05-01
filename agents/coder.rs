//! Coder Agent — Receives file paths + requirements. Outputs diff patches.
//!
//! This is a reference implementation of an ADP agent. It compiles to
//! a WASM module and runs inside the [`adp-runtime`] sandbox.
//!
//! # Input Context
//!
//! ```json
//! {
//!   "files": ["src/main.rs", "src/lib.rs"],
//!   "requirements": "Add error handling to all public functions"
//! }
//! ```
//!
//! # Output
//!
//! ```json
//! {
//!   "patches": [
//!     {
//!       "file": "src/main.rs",
//!       "diff": "--- a/src/main.rs
+++ b/src/main.rs
..."
//!     }
//!   ]
//! }
//! ```

use serde::{Deserialize, Serialize};

/// Input to the coder agent.
#[derive(Debug, Deserialize)]
pub struct CoderInput {
    pub files: Vec<String>,
    pub requirements: String,
}

/// Output from the coder agent.
#[derive(Debug, Serialize)]
pub struct CoderOutput {
    pub patches: Vec<Patch>,
}

#[derive(Debug, Serialize)]
pub struct Patch {
    pub file: String,
    pub diff: String,
}

/// Main entrypoint for the coder agent.
///
/// In production, this is compiled to WASM and called by the runtime.
/// The input is read from a host function; the output is written via
/// a host function.
#[no_mangle]
pub extern "C" fn _start() {
    // Placeholder: in a real implementation, this would:
    // 1. Read input JSON from host (via WASI stdin or host function)
    // 2. Parse the input into CoderInput
    // 3. Read the specified files (via capability-checked host function)
    // 4. Generate diff patches based on requirements
    // 5. Write output JSON to host
}

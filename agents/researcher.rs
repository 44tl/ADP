//! Researcher Agent — Receives queries. Uses MCP browser tool. Outputs summaries.
//!
//! # Input Context
//!
//! ```json
//! {
//!   "query": "What are the latest developments in Rust async runtimes?",
//!   "sources": ["https://news.ycombinator.com", "https://lobste.rs"]
//! }
//! ```
//!
//! # Output
//!
//! ```json
//! {
//!   "summary": "...",
//!   "sources": ["https://..."],
//!   "confidence": 0.85
//! }
//! ```

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ResearcherInput {
    pub query: String,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ResearcherOutput {
    pub summary: String,
    pub sources: Vec<String>,
    pub confidence: f32,
}

#[no_mangle]
pub extern "C" fn _start() {
    // Placeholder: would use MCP browser tool via host function
}

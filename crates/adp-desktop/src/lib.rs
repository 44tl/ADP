//! ADP Desktop — Tauri v2 application shell.
//!
//! This crate provides the desktop UI for ADP. It is a thin wrapper around
//! the Tauri framework, exposing ADP core functionality to a React + TypeScript
//! + Tailwind frontend.
//!
//! # Architecture
//!
//! - [`commands`] — Tauri command handlers that bridge JS → Rust.
//! - [`state`] — Application state management (shared across commands).
//! - [`menu`] — Native menu bar definitions.

pub mod commands;
pub mod error;
pub mod menu;
pub mod state;

pub use error::{DesktopError, Result};
pub use state::AppState;

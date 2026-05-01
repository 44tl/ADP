//! ADP Core — Protocol definitions, DAG scheduler, and state machine.
//!
//! This crate contains the foundational types and logic for the Agent Delegation Protocol.
//! It is strictly local-first: no network I/O, no cloud dependencies in the core path.
//!
//! # Architecture
//!
//! - [`task`] — Core domain types: [`Task`], [`TaskState`], [`Strategy`], [`Event`].
//! - [`state_machine`] — Pure transition logic. Validates every state change and emits audit events.
//! - [`scheduler`] — [`DagScheduler`] orchestrates multiple tasks, handles parent-child blocking,
//!   and implements strategy-specific lifecycle rules.
//! - [`store`] — Persistence traits ([`TaskStore`], [`EventStore`]) and in-memory implementations
//!   for testing.
//! - [`store_libsql`] — Production [`TaskStore`] / [`EventStore`] backed by libSQL (Turso embedded).
//! - [`db`] — Database connection and migration management.
//! - [`error`] — Error types using `thiserror`.

pub mod db;
pub mod error;
pub mod scheduler;
pub mod state_machine;
pub mod store;
pub mod store_libsql;
pub mod task;

pub use error::{AdpError, Result};
pub use task::{Event, EventType, Id, Strategy, Task, TaskState};

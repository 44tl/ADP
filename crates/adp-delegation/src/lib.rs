//! ADP Delegation — Delegation strategies, agent discovery, and consensus.
//!
//! This crate sits above [`adp-core`] and [`adp-runtime`]. It is responsible for:
//! - Deciding **which** agent(s) receive a task based on the task's [`Strategy`](adp_core::task::Strategy).
//! - Managing agent discovery and capability matching.
//! - Implementing consensus rules for [`VOTE`](adp_core::task::Strategy::Vote) strategy.
//! - Coordinating [`BROADCAST`](adp_core::task::Strategy::Broadcast) cancellation of losers.
//!
//! # Architecture
//!
//! - [`strategy`] — Trait-based strategy implementations (Single, Broadcast, Vote, Pipeline, MapReduce).
//! - [`registry`] — Agent discovery and capability-based matching.
//! - [`consensus`] — Quorum-based agreement for Vote strategy.
//! - [`engine`] — [`DelegationEngine`] ties everything together.

pub mod consensus;
pub mod engine;
pub mod error;
pub mod registry;
pub mod strategy;

pub use engine::DelegationEngine;
pub use error::{DelegationError, Result};
pub use registry::{AgentRegistry, RegistryEntry};
pub use strategy::{DelegationStrategy, StrategyContext};

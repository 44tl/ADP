//! ADP Gateway — gRPC/REST API and WebSocket streaming.
//!
//! Exposes the ADP core functionality over network protocols:
//! - gRPC (tonic) for internal service communication.
//! - REST (axum) for external clients.
//! - WebSocket for real-time event streaming.
//!
//! # Security
//!
//! The gateway does not implement authentication itself. It expects to run
//! behind a reverse proxy (e.g., nginx, traefik) that handles TLS and auth.

pub mod error;
pub mod grpc;
pub mod rest;
pub mod websocket;

pub use error::{GatewayError, Result};

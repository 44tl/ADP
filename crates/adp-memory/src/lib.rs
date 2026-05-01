//! ADP Memory — Vector storage, conversation history, and context windows.
//!
//! Uses Qdrant in embedded mode for local vector search. No external
//! vector database server required.
//!
//! # Architecture
//!
//! - [`vector_store`] — Qdrant-backed vector storage for agent memory/RAG.
//! - [`conversation`] — Conversation history management with sliding context windows.
//! - [`context_window`] — Token-bounded context window trimming.

pub mod context_window;
pub mod conversation;
pub mod error;
pub mod vector_store;

pub use context_window::{ContextWindow, WindowConfig};
pub use conversation::{Conversation, ConversationStore};
pub use error::{MemoryError, Result};
pub use vector_store::{MemoryEntry, VectorStore};

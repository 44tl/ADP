//! Vector store for agent memory and RAG.
//!
//! Uses Qdrant in embedded mode. Each agent gets its own collection.
//! Memories are stored as vectors with metadata for filtering.

use crate::error::{MemoryError, Result};
use adp_core::task::Id;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, instrument};

/// A memory entry to be stored in the vector database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: Id,
    pub agent_id: Option<Id>,
    pub task_id: Option<Id>,
    pub content: String,
    pub embedding: Vec<f32>,
    pub metadata: HashMap<String, String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Vector store backed by Qdrant (embedded mode).
#[derive(Debug, Clone)]
pub struct VectorStore {
    db_path: std::path::PathBuf,
    dimension: usize,
}

impl VectorStore {
    /// Create or open a vector store at the given path.
    pub fn new<P: AsRef<Path>>(path: P, dimension: usize) -> Result<Self> {
        let db_path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&db_path).map_err(|e| {
            MemoryError::VectorStore(format!("failed to create db directory: {e}"))
        })?;

        info!(path = %db_path.display(), dimension, "vector store opened");
        Ok(Self { db_path, dimension })
    }

    /// Store a memory entry.
    #[instrument(skip(self, entry), fields(entry_id = %entry.id))]
    pub async fn store(&self, entry: &MemoryEntry) -> Result<()> {
        // In production, this would call Qdrant's upsert API.
        // For now, we serialize to a local JSONL file as a placeholder.
        let collection = self.collection_path(&entry.agent_id);
        let line = serde_json::to_string(entry)
            .map_err(|e| MemoryError::Serialization(e.to_string()))?;

        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&collection)
            .await
            .map_err(|e| MemoryError::VectorStore(format!("file open: {e}")))?;

        file.write_all(line.as_bytes()).await.map_err(|e| {
            MemoryError::VectorStore(format!("write failed: {e}"))
        })?;
        file.write_all(b"
").await.map_err(|e| {
            MemoryError::VectorStore(format!("write failed: {e}"))
        })?;

        debug!(entry_id = %entry.id, "memory stored");
        Ok(())
    }

    /// Search for memories similar to the given query vector.
    #[instrument(skip(self, query_vector), fields(limit = limit))]
    pub async fn search(
        &self,
        agent_id: Option<Id>,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        // Placeholder: in production, this would call Qdrant's search API.
        // For now, return empty results.
        debug!("vector search executed (placeholder)");
        Ok(Vec::new())
    }

    /// Delete a memory entry by ID.
    #[instrument(skip(self), fields(entry_id = %id))]
    pub async fn delete(&self, id: Id) -> Result<()> {
        debug!(entry_id = %id, "memory delete (placeholder)");
        Ok(())
    }

    fn collection_path(&self, agent_id: &Option<Id>) -> std::path::PathBuf {
        let name = agent_id
            .map(|id| format!("agent_{}", id))
            .unwrap_or_else(|| "global".to_string());
        self.db_path.join(format!("{}.jsonl", name))
    }
}

//! Conversation history management.
//!
//! [`Conversation`] tracks message history for an agent with automatic
//! context window management.

use crate::context_window::{ContextWindow, WindowConfig};
use crate::error::{MemoryError, Result};
use adp_core::task::Id;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Conversation with sliding context window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Id,
    pub agent_id: Id,
    pub messages: VecDeque<Message>,
    pub window: ContextWindow,
}

impl Conversation {
    /// Create a new conversation for an agent.
    pub fn new(agent_id: Id, window_config: WindowConfig) -> Self {
        Self {
            id: Id::new(),
            agent_id,
            messages: VecDeque::new(),
            window: ContextWindow::new(window_config),
        }
    }

    /// Append a message and trim the context window if needed.
    pub fn push(&mut self, message: Message) -> Result<()> {
        self.messages.push_back(message);
        self.window.trim(&mut self.messages)?;
        Ok(())
    }

    /// Get the current messages (after window trimming).
    pub fn messages(&self) -> &VecDeque<Message> {
        &self.messages
    }

    /// Serialize to JSON for persistence.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|e| MemoryError::Serialization(e.to_string()))
    }
}

/// Store for persisting conversations.
#[derive(Debug, Clone, Default)]
pub struct ConversationStore {
    // In production, backed by libSQL. For now, in-memory.
    conversations: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<Id, Conversation>>>,
}

impl ConversationStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn save(&self, conversation: &Conversation) -> Result<()> {
        let mut convs = self.conversations.write().await;
        convs.insert(conversation.id, conversation.clone());
        Ok(())
    }

    pub async fn get(&self, id: &Id) -> Result<Option<Conversation>> {
        let convs = self.conversations.read().await;
        Ok(convs.get(id).cloned())
    }

    pub async fn get_by_agent(&self, agent_id: &Id) -> Result<Vec<Conversation>> {
        let convs = self.conversations.read().await;
        Ok(convs
            .values()
            .filter(|c| c.agent_id == *agent_id)
            .cloned()
            .collect())
    }
}

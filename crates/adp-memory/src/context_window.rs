//! Token-bounded context window management.
//!
//! [`ContextWindow`] trims conversation history to stay within a token
//! budget, preserving system messages and recent context.

use crate::error::{MemoryError, Result};
use crate::conversation::Message;
use std::collections::VecDeque;

/// Configuration for context window trimming.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowConfig {
    /// Maximum tokens in the context window.
    pub max_tokens: usize,
    /// Tokens reserved for the response.
    pub response_tokens: usize,
    /// Approximate tokens per character (rough heuristic: 1 token ≈ 4 chars).
    pub tokens_per_char: f32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            max_tokens: 4096,
            response_tokens: 1024,
            tokens_per_char: 0.25,
        }
    }
}

/// Manages context window trimming for conversations.
#[derive(Debug, Clone)]
pub struct ContextWindow {
    config: WindowConfig,
}

impl ContextWindow {
    pub fn new(config: WindowConfig) -> Self {
        Self { config }
    }

    /// Trim messages to fit within the token budget.
    ///
    /// Strategy:
    /// 1. Always keep system messages.
    /// 2. Always keep the most recent user message.
    /// 3. Drop oldest non-system messages first.
    pub fn trim(&self, messages: &mut VecDeque<Message>) -> Result<()> {
        let available = self.config.max_tokens.saturating_sub(self.config.response_tokens);

        loop {
            let current_tokens = self.estimate_tokens(messages);
            if current_tokens <= available {
                break;
            }

            // Find the oldest non-system, non-last message to drop.
            let drop_idx = messages
                .iter()
                .enumerate()
                .rev()
                .skip(1) // Never drop the last message
                .find(|(_, m)| m.role != crate::conversation::Role::System)
                .map(|(i, _)| i);

            match drop_idx {
                Some(idx) => {
                    messages.remove(idx);
                }
                None => {
                    // Can't trim any further without dropping system or last message.
                    return Err(MemoryError::ContextWindowExceeded(format!(
                        "messages exceed token budget: {} > {}",
                        current_tokens, available
                    )));
                }
            }
        }

        Ok(())
    }

    fn estimate_tokens(&self, messages: &VecDeque<Message>) -> usize {
        let total_chars: usize = messages.iter().map(|m| m.content.len()).sum();
        (total_chars as f32 * self.config.tokens_per_char).ceil() as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::{Message, Role};

    fn msg(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
            metadata: None,
        }
    }

    #[test]
    fn trim_drops_oldest_non_system() {
        let config = WindowConfig {
            max_tokens: 100,
            response_tokens: 10,
            tokens_per_char: 1.0, // 1 char = 1 token for easy math
        };
        let window = ContextWindow::new(config);
        let mut messages = VecDeque::from([
            msg(Role::System, "system"),
            msg(Role::User, "a".repeat(50)),
            msg(Role::Assistant, "b".repeat(50)),
            msg(Role::User, "c".repeat(50)), // last message, must keep
        ]);

        window.trim(&mut messages).unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].content, "system");
        assert_eq!(messages[1].content, "b".repeat(50));
        assert_eq!(messages[2].content, "c".repeat(50));
    }

    #[test]
    fn keeps_system_messages() {
        let config = WindowConfig {
            max_tokens: 50,
            response_tokens: 10,
            tokens_per_char: 1.0,
        };
        let window = ContextWindow::new(config);
        let mut messages = VecDeque::from([
            msg(Role::System, "system instructions here"),
            msg(Role::User, "a".repeat(100)),
        ]);

        window.trim(&mut messages).unwrap();
        assert!(messages.iter().any(|m| m.role == Role::System));
    }
}

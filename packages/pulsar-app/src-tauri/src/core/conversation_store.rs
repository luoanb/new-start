use super::{
    error::{AppError, AppResult},
    models::{Conversation, ConversationMode, Message, MessageRole},
    storage,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone)]
pub struct ConversationStore {
    root: PathBuf,
    sessions_dir: PathBuf,
}

impl ConversationStore {
    pub fn default() -> AppResult<Self> {
        let root = storage::default_root()?;
        Self::new(root)
    }

    pub fn new(root: impl Into<PathBuf>) -> AppResult<Self> {
        let root = root.into();
        let sessions_dir = root.join("sessions");
        fs::create_dir_all(&sessions_dir)?;

        Ok(Self { root, sessions_dir })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create_conversation(
        &self,
        conversation_id: Option<String>,
        mode: ConversationMode,
    ) -> AppResult<Conversation> {
        let now = now_ms();
        let id = match conversation_id {
            Some(id) if !id.trim().is_empty() => id,
            Some(_) => {
                return Err(AppError::InvalidInput(
                    "Conversation id cannot be empty".into(),
                ))
            }
            None => new_conversation_id(),
        };

        let conversation = Conversation {
            id,
            mode,
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
            extra: None,
        };

        self.save_conversation(&conversation)?;
        Ok(conversation)
    }

    pub fn get_conversation(&self, conversation_id: &str) -> AppResult<Option<Conversation>> {
        if conversation_id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Conversation id cannot be empty".into(),
            ));
        }

        let path = self.conversation_path(conversation_id);
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path)?;
        Ok(Some(serde_json::from_str(&content)?))
    }

    pub fn require_conversation(&self, conversation_id: &str) -> AppResult<Conversation> {
        self.get_conversation(conversation_id)?
            .ok_or_else(|| AppError::ConversationNotFound(conversation_id.to_string()))
    }

    pub fn list_conversations(&self) -> AppResult<Vec<Conversation>> {
        let mut conversations = Vec::new();

        for entry in fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }

            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };

            if let Ok(conversation) = serde_json::from_str::<Conversation>(&content) {
                conversations.push(conversation);
            }
        }

        conversations.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(conversations)
    }

    pub fn add_message(&self, conversation_id: &str, message: Message) -> AppResult<Conversation> {
        let mut conversation = match self.get_conversation(conversation_id)? {
            Some(conversation) => conversation,
            None => {
                self.create_conversation(Some(conversation_id.to_string()), ConversationMode::Chat)?
            }
        };

        conversation.messages.push(message);
        conversation.updated_at = now_ms();
        self.save_conversation(&conversation)?;
        Ok(conversation)
    }

    /// 增量落库：定位最后一条 assistant 消息并应用 `patch`（流式场景单会话唯一），读改写后全量写盘。
    ///
    /// - 找不到 assistant 消息时返回 `Ok(conversation)` 不变更（容错，不中断流式）。
    /// - 写盘频率由调用方节流控制（首 chunk 立即写 → ~150ms 节流 → 完成时最终写）。
    pub fn update_last_assistant_message(
        &self,
        conversation_id: &str,
        patch: impl FnOnce(&mut Message),
    ) -> AppResult<Conversation> {
        let mut conversation = self.require_conversation(conversation_id)?;
        let Some(message) = conversation
            .messages
            .iter_mut()
            .rev()
            .find(|m| m.role == MessageRole::Assistant)
        else {
            return Ok(conversation);
        };
        patch(message);
        conversation.updated_at = now_ms();
        self.save_conversation(&conversation)?;
        Ok(conversation)
    }

    pub fn save_conversation(&self, conversation: &Conversation) -> AppResult<()> {
        let path = self.conversation_path(&conversation.id);
        let content = serde_json::to_string_pretty(conversation)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn clear_conversation(&self, conversation_id: &str) -> AppResult<()> {
        if conversation_id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Conversation id cannot be empty".into(),
            ));
        }

        let path = self.conversation_path(conversation_id);
        if !path.exists() {
            return Err(AppError::ConversationNotFound(conversation_id.to_string()));
        }

        fs::remove_file(path)?;
        Ok(())
    }

    fn conversation_path(&self, conversation_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{conversation_id}.json"))
    }
}

pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis()
}

fn new_conversation_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    format!("conv_{nanos}")
}

#[cfg(test)]
mod tests {
    use crate::core::models::MessageBody;
    use super::*;

    fn text_message(role: MessageRole, content: &str) -> Message {
        Message {
            role,
            body: MessageBody::Text {
                content: content.to_string(),
                reasoning: None,
                tool_calls: None,
            },
            timestamp: now_ms(),
            neuron_id: None,
        }
    }

    fn test_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join("pulsar-app-tests")
            .join(name)
            .join(format!("{}", now_ms()))
    }

    #[test]
    fn update_last_assistant_message_patches_text() {
        let store = ConversationStore::new(test_root("update_assistant_patches")).unwrap();
        let conv = store
            .create_conversation(None, ConversationMode::Chat)
            .unwrap();
        store
            .add_message(&conv.id, text_message(MessageRole::User, "hi"))
            .unwrap();
        store
            .add_message(
                &conv.id,
                Message {
                    role: MessageRole::Assistant,
                    body: MessageBody::Text {
                        content: String::new(),
                        reasoning: Some("think".into()),
                        tool_calls: None,
                    },
                    timestamp: now_ms(),
                    neuron_id: None,
                },
            )
            .unwrap();

        let updated = store
            .update_last_assistant_message(&conv.id, |m| {
                if let MessageBody::Text { content, .. } = &mut m.body {
                    content.push_str("hello");
                }
            })
            .unwrap();
        let last = updated.messages.last().unwrap();
        assert_eq!(last.text(), "hello");
        assert_eq!(last.reasoning(), Some("think"));
    }

    #[test]
    fn update_last_assistant_message_no_assistant_is_noop() {
        let store = ConversationStore::new(test_root("update_assistant_noop")).unwrap();
        let conv = store
            .create_conversation(None, ConversationMode::Chat)
            .unwrap();
        store
            .add_message(&conv.id, text_message(MessageRole::User, "hi"))
            .unwrap();

        let updated = store
            .update_last_assistant_message(&conv.id, |m| {
                if let MessageBody::Text { content, .. } = &mut m.body {
                    content.push_str("x");
                }
            })
            .unwrap();
        // 无 assistant 消息：不变更，不误伤 user 消息。
        assert_eq!(updated.messages.last().unwrap().text(), "hi");
        assert_eq!(updated.messages.len(), 1);
    }

    #[test]
    fn update_last_assistant_message_empty_conversation_is_noop() {
        let store = ConversationStore::new(test_root("update_assistant_empty")).unwrap();
        let conv = store
            .create_conversation(None, ConversationMode::Chat)
            .unwrap();
        let updated = store
            .update_last_assistant_message(&conv.id, |_| {})
            .unwrap();
        assert!(updated.messages.is_empty());
    }
}

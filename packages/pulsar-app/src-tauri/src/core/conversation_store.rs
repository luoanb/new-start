use super::{
    error::{AppError, AppResult},
    models::{Conversation, ConversationMode, Message, MessageRole},
    storage,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

/// 会话存储：全部读改写通过可重入互斥锁串行化，杜绝流式写盘与新消息写入的
/// 「读-改-写交错」lost update（用户消息被吞的根因）。锁可重入：组合方法
/// （create/add_message/update_last_assistant_message）内部再调 save_conversation 不死锁。
#[derive(Debug, Clone)]
pub struct ConversationStore {
    root: PathBuf,
    sessions_dir: PathBuf,
    lock: Arc<parking_lot::ReentrantMutex<()>>,
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

        Ok(Self {
            root,
            sessions_dir,
            lock: Arc::new(parking_lot::ReentrantMutex::new(())),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create_conversation(
        &self,
        conversation_id: Option<String>,
        mode: ConversationMode,
    ) -> AppResult<Conversation> {
        let _guard = self.lock.lock();
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
        let _guard = self.lock.lock();
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
        let _guard = self.lock.lock();
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
        let _guard = self.lock.lock();
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
        let _guard = self.lock.lock();
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

    /// 增量落库：按消息索引应用 `patch`（读改写后全量写盘）。
    ///
    /// 与 `update_last_assistant_message` 相比按索引精确定位——用于流式中断收敛等场景：
    /// 抢占后新轮可能已追加用户消息与占位消息，「最后一条 assistant」不再是本轮的占位，
    /// 必须按本轮记录的索引更新，避免写错消息。
    pub fn update_message_at(
        &self,
        conversation_id: &str,
        index: usize,
        patch: impl FnOnce(&mut Message),
    ) -> AppResult<Conversation> {
        let _guard = self.lock.lock();
        let mut conversation = self.require_conversation(conversation_id)?;
        let Some(message) = conversation.messages.get_mut(index) else {
            return Ok(conversation);
        };
        patch(message);
        conversation.updated_at = now_ms();
        self.save_conversation(&conversation)?;
        Ok(conversation)
    }

    pub fn save_conversation(&self, conversation: &Conversation) -> AppResult<()> {
        let _guard = self.lock.lock();
        let path = self.conversation_path(&conversation.id);
        let content = serde_json::to_string_pretty(conversation)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn clear_conversation(&self, conversation_id: &str) -> AppResult<()> {
        let _guard = self.lock.lock();
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

    /// 回归（消息被吞）：模拟「流式节流写盘」与「用户新消息追加」并发交错。
    /// 加锁前读-改-写全量写盘竞态会互相覆盖（lost update），用户消息偶发丢失；
    /// 加锁后二者原子化，断言所有用户消息都在、占位消息无中间态覆盖。
    #[test]
    fn concurrent_stream_write_and_user_add_do_not_lose_messages() {
        let store = Arc::new(
            ConversationStore::new(test_root("concurrent_stream_write")).unwrap(),
        );
        let conv = store
            .create_conversation(None, ConversationMode::Chat)
            .unwrap();
        // 占位（index 0）：模拟流式轮的占位消息。
        store
            .add_message(&conv.id, text_message(MessageRole::Assistant, ""))
            .unwrap();
        let id = conv.id.clone();

        let writer = {
            let store = Arc::clone(&store);
            let id = id.clone();
            std::thread::spawn(move || {
                for i in 0..100 {
                    store
                        .update_message_at(&id, 0, |m| {
                            if let MessageBody::Text { content, .. } = &mut m.body {
                                *content = format!("partial-{i}");
                            }
                        })
                        .unwrap();
                }
            })
        };
        let adder = {
            let store = Arc::clone(&store);
            let id = id.clone();
            std::thread::spawn(move || {
                for i in 0..100 {
                    store
                        .add_message(
                            &id,
                            text_message(MessageRole::User, &format!("user-{i}")),
                        )
                        .unwrap();
                }
            })
        };
        writer.join().unwrap();
        adder.join().unwrap();

        let conv = store.get_conversation(&id).unwrap().unwrap();
        let user_count = conv
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .count();
        assert_eq!(user_count, 100, "no user message may be lost");
        assert_eq!(conv.messages.len(), 101);
        // 占位消息应为某次原子写盘的完整结果（无中间态拼接）。
        assert!(
            conv.messages[0].text().starts_with("partial-"),
            "placeholder must be a complete atomic write, got: {:?}",
            conv.messages[0].text()
        );
    }

    /// 回归（占位污染）：抢占后新轮已追加用户消息与新的占位消息，
    /// 旧轮的中断收敛写必须按索引更新自己的占位，不得污染新轮的占位。
    #[test]
    fn streaming_write_by_index_does_not_overwrite_new_placeholder() {
        let store = ConversationStore::new(test_root("stream_write_indexed")).unwrap();
        let conv = store
            .create_conversation(None, ConversationMode::Chat)
            .unwrap();
        let id = conv.id.clone();
        // 旧轮：占位 index 0。
        store
            .add_message(&id, text_message(MessageRole::Assistant, ""))
            .unwrap();
        // 新轮：用户消息 index 1 + 新占位 index 2。
        store
            .add_message(&id, text_message(MessageRole::User, "new question"))
            .unwrap();
        store
            .add_message(&id, text_message(MessageRole::Assistant, ""))
            .unwrap();
        // 旧轮中断收敛写：只能作用于 index 0。
        store
            .update_message_at(&id, 0, |m| {
                if let MessageBody::Text { content, .. } = &mut m.body {
                    *content = "old partial".into();
                }
            })
            .unwrap();
        let conv = store.require_conversation(&id).unwrap();
        assert_eq!(conv.messages[0].text(), "old partial");
        assert_eq!(conv.messages[1].text(), "new question");
        assert_eq!(
            conv.messages[2].text(),
            "",
            "new placeholder must not be polluted by old round's final write"
        );
    }
}

use super::{
    error::{AppError, AppResult},
    models::{
        Conversation, ConversationMode, ConversationSummary, ConversationSummaryPage, Message,
        MessageBody, MessagePage, MessageRole,
    },
    storage,
};
use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
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

    /// 会话列表摘要分页：只读元信息（消息条数 + 首条文本摘要），不解析/传输消息正文。
    ///
    /// 排序与 `list_conversations` 一致（`updated_at` 倒序）；分页为页码制
    /// （前端追加加载时 page = 已加载条数 / page_size），`has_more` 指示是否还有更早会话。
    pub fn list_conversation_summaries(
        &self,
        page: usize,
        page_size: usize,
    ) -> AppResult<ConversationSummaryPage> {
        let _guard = self.lock.lock();
        let mut summaries = Vec::new();

        for entry in fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(light) = serde_json::from_str::<ConversationLight>(&content) else {
                continue;
            };
            summaries.push(ConversationSummary {
                id: light.id,
                mode: light.mode,
                message_count: light.messages.count,
                preview: light.messages.first_text,
                created_at: light.created_at,
                updated_at: light.updated_at,
                extra: light.extra,
            });
        }

        summaries.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        let total = summaries.len();
        let start = page.saturating_mul(page_size);
        let end = start.saturating_add(page_size).min(total);
        let items = if start >= total {
            Vec::new()
        } else {
            summaries[start..end].to_vec()
        };
        Ok(ConversationSummaryPage {
            items,
            total,
            has_more: end < total,
        })
    }

    /// 轻量统计会话数量（仅统计 `sessions/*.json` 文件数，不解析内容；`status` 等轻量场景用）。
    pub fn conversation_count(&self) -> AppResult<usize> {
        let _guard = self.lock.lock();
        let mut count = 0usize;
        for entry in fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|v| v.to_str()) == Some("json") {
                count += 1;
            }
        }
        Ok(count)
    }

    /// 消息历史分页：从最新倒推切片（`offset` = 已加载条数，`limit` = 本次条数），
    /// 避免整段历史全量返回；`has_more` 指示是否还有更早消息。
    pub fn history_page(
        &self,
        conversation_id: &str,
        limit: usize,
        offset: usize,
    ) -> AppResult<MessagePage> {
        let _guard = self.lock.lock();
        let conversation = self.require_conversation(conversation_id)?;
        let total = conversation.messages.len();
        let limit = limit.max(1);
        let end = total.saturating_sub(offset);
        let start = end.saturating_sub(limit);
        let messages = conversation.messages[start..end].to_vec();
        Ok(MessagePage {
            messages,
            total,
            offset,
            has_more: start > 0,
        })
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

    /// 存量止损（幂等）：扫描全部会话，单条消息正文超 `max_chars` → 落库截断。
    ///
    /// 事故背景：`conv_1787253076882845861` 有一条 3MB grep 工具结果在统一截断落地前
    /// 已写入存储；L3 压缩虽会在发送时强制降级，但真相源仍滞留巨型消息。本方法
    /// 在启动时执行一次：工具结果走 `cap_tool_result`（带重试提示），其余正文走
    /// `cap_text`。返回实际被截断的消息条数（0 = 无存量超限，正常不写盘）。
    pub fn sanitize_oversized_messages(&self, max_chars: usize) -> AppResult<usize> {
        let mut trimmed = 0usize;
        for mut conversation in self.list_conversations()? {
            let mut changed = false;
            for message in conversation.messages.iter_mut() {
                let total = message.text().chars().count();
                if total <= max_chars {
                    continue;
                }
                let tool_name = match &message.body {
                    MessageBody::ToolResult { tool_name, .. } => Some(tool_name.clone()),
                    _ => None,
                };
                message.map_content(|content| match tool_name.as_deref() {
                    Some(name) => super::context_safety::cap_tool_result(name, content.to_string(), max_chars),
                    None => super::context_safety::cap_text(content, max_chars),
                });
                changed = true;
            }
            if changed {
                conversation.updated_at = now_ms();
                self.save_conversation(&conversation)?;
                trimmed += 1;
            }
        }
        Ok(trimmed)
    }

    fn conversation_path(&self, conversation_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{conversation_id}.json"))
    }
}

// ── 会话文件轻量反序列化（列表专用）──────────────────────────

/// 会话文件的轻量结构：`messages` 只产出条数 + 首条文本摘要，不保留消息正文。
#[derive(Debug, Deserialize)]
struct ConversationLight {
    id: String,
    #[serde(default)]
    mode: ConversationMode,
    #[serde(default, deserialize_with = "deserialize_message_summary")]
    messages: MessageSummarySeed,
    created_at: u128,
    updated_at: u128,
    #[serde(default)]
    extra: Option<serde_json::Value>,
}

fn deserialize_message_summary<'de, D>(deserializer: D) -> Result<MessageSummarySeed, D::Error>
where
    D: serde::Deserializer<'de>,
{
    MessageSummarySeed::deserialize(deserializer)
}

/// 首条文本摘要状态：找到摘要后，后续消息只计数、整条跳过（巨型工具结果/长正文不再解析）。
#[derive(Debug, Default)]
struct MessageSummarySeed {
    count: usize,
    first_text: Option<String>,
}

impl<'de> Deserialize<'de> for MessageSummarySeed {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SeqVisitor;

        impl<'de> Visitor<'de> for SeqVisitor {
            type Value = MessageSummarySeed;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a sequence of messages")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut seed = MessageSummarySeed::default();
                loop {
                    if seed.first_text.is_none() {
                        // 尚未取得摘要：逐条解析 role（body 按角色决定是否解析）。
                        match seq.next_element::<MessageSeed>()? {
                            Some(msg) => {
                                seed.count += 1;
                                if let Some(text) = msg.first_text() {
                                    seed.first_text = Some(text);
                                }
                            }
                            None => break,
                        }
                    } else {
                        // 已取得摘要：后续消息只计数、整条跳过。
                        match seq.next_element::<IgnoredAny>()? {
                            Some(_) => seed.count += 1,
                            None => break,
                        }
                    }
                }
                Ok(seed)
            }
        }

        deserializer.deserialize_seq(SeqVisitor)
    }
}

/// 单条消息的轻量视图：role 必读，body 仅在角色为 user/assistant 时解析（取首条摘要用）。
#[derive(Debug)]
struct MessageSeed {
    role: Option<MessageRole>,
    body: Option<serde_json::Value>,
}

impl MessageSeed {
    fn first_text(&self) -> Option<String> {
        match self.role {
            Some(MessageRole::User) | Some(MessageRole::Assistant) => {}
            _ => return None,
        }
        let body = self.body.as_ref()?;
        if body.get("kind")?.as_str() != Some("text") {
            return None;
        }
        body.get("content").and_then(|v| v.as_str()).map(str::to_owned)
    }
}

impl<'de> Deserialize<'de> for MessageSeed {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MapVisitor;

        impl<'de> Visitor<'de> for MapVisitor {
            type Value = MessageSeed;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a message object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut role: Option<MessageRole> = None;
                let mut body: Option<serde_json::Value> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "role" => role = Some(map.next_value()?),
                        "body" => {
                            // role 未知（防御字段乱序）或为 user/assistant 时解析正文；
                            // 超大工具结果/系统提示词（tool/system/compaction）整体跳过。
                            let need_body = matches!(
                                role,
                                None | Some(MessageRole::User) | Some(MessageRole::Assistant)
                            );
                            if need_body {
                                body = Some(map.next_value()?);
                            } else {
                                let _: IgnoredAny = map.next_value()?;
                            }
                        }
                        _ => {
                            let _: IgnoredAny = map.next_value()?;
                        }
                    }
                }
                Ok(MessageSeed { role, body })
            }
        }

        deserializer.deserialize_map(MapVisitor)
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

    /// 存量止损：巨型工具结果 / 超大正文落库截断；短消息不动；幂等（二次清理无变更）。
    #[test]
    fn sanitize_trims_oversized_and_leaves_short_untouched() {
        let store = ConversationStore::new(test_root("sanitize_oversized")).unwrap();
        let conv = store
            .create_conversation(None, ConversationMode::Chat)
            .unwrap();
        let id = conv.id.clone();
        // 短消息（不截断）。
        store
            .add_message(&id, text_message(MessageRole::User, "hi"))
            .unwrap();
        // 巨型工具结果（事故形态：3MB grep 结果）。
        let big_tool = "X".repeat(3_000_000);
        store
            .add_message(
                &id,
                Message {
                    role: MessageRole::Tool,
                    body: MessageBody::ToolResult {
                        tool_call_id: "call-1".into(),
                        tool_name: "grep".into(),
                        content: big_tool,
                    },
                    timestamp: now_ms(),
                    neuron_id: None,
                },
            )
            .unwrap();
        // 超大普通正文。
        let big_text = "Y".repeat(30_000);
        store
            .add_message(&id, text_message(MessageRole::User, &big_text))
            .unwrap();

        let trimmed = store.sanitize_oversized_messages(12_000).unwrap();
        assert_eq!(trimmed, 1, "one conversation had oversized messages");

        let conv = store.require_conversation(&id).unwrap();
        assert_eq!(conv.messages[0].text(), "hi", "short message untouched");
        let tool_text = conv.messages[1].text();
        assert!(tool_text.chars().count() <= 12_000, "tool result trimmed");
        assert!(
            tool_text.contains("3000000 chars") || tool_text.contains("[truncated"),
            "tool result carries truncation marker"
        );
        assert!(conv.messages[2].text().chars().count() <= 12_000, "text trimmed");

        // 幂等：二次清理不再返回截断会话。
        let again = store.sanitize_oversized_messages(12_000).unwrap();
        assert_eq!(again, 0, "second run is a no-op");
    }

    #[test]
    fn history_page_slices_from_latest() {
        let store = ConversationStore::new(test_root("history_page")).unwrap();
        let conv = store
            .create_conversation(None, ConversationMode::Chat)
            .unwrap();
        for (i, role) in [
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::User,
        ]
        .into_iter()
        .enumerate()
        {
            store
                .add_message(&conv.id, text_message(role, &format!("m{i}")))
                .unwrap();
        }

        // offset=0：最新一页（末尾 2 条），has_more=true。
        let p0 = store.history_page(&conv.id, 2, 0).unwrap();
        assert_eq!(p0.total, 5);
        assert_eq!(
            p0.messages.iter().map(|m| m.text()).collect::<Vec<_>>(),
            ["m3", "m4"]
        );
        assert!(p0.has_more);

        // offset=2：倒推第 3-4 条。
        let p1 = store.history_page(&conv.id, 2, 2).unwrap();
        assert_eq!(
            p1.messages.iter().map(|m| m.text()).collect::<Vec<_>>(),
            ["m1", "m2"]
        );
        assert!(p1.has_more);

        // offset=4：最老 1 条，has_more=false。
        let p2 = store.history_page(&conv.id, 2, 4).unwrap();
        assert_eq!(
            p2.messages.iter().map(|m| m.text()).collect::<Vec<_>>(),
            ["m0"]
        );
        assert!(!p2.has_more);

        // offset 超界：返回空页且不再有更多。
        let p3 = store.history_page(&conv.id, 2, 99).unwrap();
        assert!(p3.messages.is_empty());
        assert!(!p3.has_more);
    }

    #[test]
    fn list_conversation_summaries_light_parse_and_paginate() {
        let store = ConversationStore::new(test_root("summary_light")).unwrap();
        // 会话 A：首条即巨型工具结果（应整体跳过，不参与摘要），随后是首条 user 文本。
        let a = store
            .create_conversation(None, ConversationMode::Chat)
            .unwrap();
        store
            .add_message(
                &a.id,
                Message {
                    role: MessageRole::Tool,
                    body: MessageBody::ToolResult {
                        tool_call_id: "t1".into(),
                        tool_name: "grep".into(),
                        content: "T".repeat(50_000),
                    },
                    timestamp: now_ms(),
                    neuron_id: None,
                },
            )
            .unwrap();
        store
            .add_message(&a.id, text_message(MessageRole::User, "hello world"))
            .unwrap();
        // 会话 B：无 user/assistant 文本（纯工具），preview 应为 None。
        let b = store
            .create_conversation(None, ConversationMode::Chat)
            .unwrap();
        store
            .add_message(
                &b.id,
                Message {
                    role: MessageRole::Tool,
                    body: MessageBody::ToolResult {
                        tool_call_id: "t2".into(),
                        tool_name: "read".into(),
                        content: "R".repeat(50_000),
                    },
                    timestamp: now_ms(),
                    neuron_id: None,
                },
            )
            .unwrap();

        // 第 0 页：最新（b 在后创建 → updated_at 更大排前）1 条。
        let page0 = store.list_conversation_summaries(0, 1).unwrap();
        assert_eq!(page0.total, 2);
        assert_eq!(page0.items.len(), 1);
        assert!(page0.has_more);
        assert_eq!(page0.items[0].id, b.id);
        assert_eq!(page0.items[0].message_count, 1);
        assert_eq!(page0.items[0].preview, None, "纯工具会话无文本摘要");

        // 第 1 页：会话 A，preview = 首条 user 文本（巨型工具结果被跳过，未误取）。
        let page1 = store.list_conversation_summaries(1, 1).unwrap();
        assert_eq!(page1.items.len(), 1);
        assert!(!page1.has_more);
        assert_eq!(page1.items[0].id, a.id);
        assert_eq!(page1.items[0].message_count, 2);
        assert_eq!(page1.items[0].preview.as_deref(), Some("hello world"));
    }

    #[test]
    fn conversation_count_counts_files_only() {
        let store = ConversationStore::new(test_root("conversation_count")).unwrap();
        store.create_conversation(None, ConversationMode::Chat).unwrap();
        store.create_conversation(None, ConversationMode::Chat).unwrap();
        assert_eq!(store.conversation_count().unwrap(), 2);
    }
}

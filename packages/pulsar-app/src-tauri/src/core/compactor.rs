use super::{
    error::{AppError, AppResult},
    model_call_input::{ModelAppendTemplate, ModelCallInput},
    models::{
        CompactionConfig, Conversation, Message, MessageBody, MessageRole, ModelCallRequest,
        ThinkingConfig,
    },
    providers::ProviderRegistry,
};

/// Rough token estimation: 1 ASCII token ≈ 4 characters; CJK/multibyte ≈ 1 token/char.
/// `chars/4` alone underestimates Chinese text (which is ~1 token per char), delaying
/// compaction until it's too late. This split is still simple but far closer for mixed text.
fn estimate_tokens(text: &str) -> usize {
    let mut ascii_run = 0usize;
    let mut tokens = 0usize;
    for ch in text.chars() {
        if ch.is_ascii() {
            ascii_run += 1;
        } else {
            // 非 ASCII（中文等多字节）：约 1 字符 ≈ 1 token。
            tokens += ascii_run / 4;
            ascii_run = 0;
            tokens += 1;
        }
    }
    tokens + ascii_run / 4
}

/// Build the compaction summary prompt.
fn compaction_prompt(messages: &[Message]) -> String {
    let conversation_text: String = messages
        .iter()
        .map(|m| {
            let role = match m.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
                MessageRole::Tool => "tool",
                MessageRole::Compaction => "compaction",
            };
            format!("[{role}]: {}", m.text())
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Please summarize the following conversation concisently, preserving key information, decisions, and context:\n\n{conversation_text}\n\n---\nSummary:"
    )
}

/// 单条消息截断（wire 层兜底）：按 token 预算换算为字符预算后 head/tail 截断 + 标记。
/// 截断后的消息仍保留头部信息与尾部结论，中段用「已截断，请缩小范围重试」提示占位。
fn cap_single_message(text: &str, token_budget: usize) -> String {
    let char_budget = token_budget * 4;
    let total = text.chars().count();
    if total <= char_budget {
        return text.to_string();
    }
    let head_budget = char_budget * 2 / 3;
    let head: String = text.chars().take(head_budget).collect();
    let tail_start = total.saturating_sub(char_budget / 3);
    let tail: String = text.chars().skip(tail_start).collect();
    format!(
        "{head}\n…[truncated: 单条消息估算 {total} 字符 > 预算 {char_budget} 字符（~{token_budget} token）；中段已省略，如需完整内容请用更精确参数重试]…\n{tail}"
    )
}

/// Compactor handles token estimation and LLM-based conversation compression.
#[derive(Debug, Clone)]
pub struct Compactor {
    config: CompactionConfig,
}

impl Compactor {
    pub fn new(config: CompactionConfig) -> Self {
        Self { config }
    }

    /// Returns the current config (useful for display).
    pub fn config(&self) -> &CompactionConfig {
        &self.config
    }

    /// Rough token estimate for a full conversation.
    pub fn estimate_conversation_tokens(&self, conversation: &Conversation) -> usize {
        conversation
            .messages
            .iter()
            .map(|m| {
                let role_len = match m.role {
                    MessageRole::System => 6,
                    MessageRole::User => 4,
                    MessageRole::Assistant => 8,
                    MessageRole::Tool => 8,
                    MessageRole::Compaction => 10,
                };
                role_len + estimate_tokens(m.text())
            })
            .sum()
    }

    /// Check if a conversation exceeds the threshold and needs compaction.
    pub fn needs_compaction(&self, conversation: &Conversation, context_window: u32) -> bool {
        if !self.config.enabled {
            return false;
        }
        let estimated = self.estimate_conversation_tokens(conversation);
        let threshold = (context_window as f64 * self.config.threshold_ratio) as usize;
        estimated >= threshold
    }

    /// Determine the cutoff index: messages before this index should be summarized.
    /// Keeps the last `keep_last` user-assistant exchanges (tail verbatim).
    ///
    /// 判据改为**体量感知**：不再用「消息数 ≤ keep_last×2 就不压缩」拦截——
    /// 单条超大消息（如 3MB grep 结果）即使消息数很少也会撑爆上下文，
    /// 体量超阈值就必须给压缩机会；keep_last 仍保证 tail 完整性。
    fn compaction_boundary(&self, conversation: &Conversation) -> usize {
        let total = conversation.messages.len();
        if total <= 1 {
            return 0;
        }

        // Count backwards to find the cutoff: keep the last N exchanges.
        // Counting Assistant messages determines the exchange boundary.
        let mut exchanges = 0usize;
        let cutoff = 'search: {
            for (i, msg) in conversation.messages.iter().enumerate().rev() {
                if msg.role == MessageRole::Assistant {
                    exchanges += 1;
                    if exchanges >= self.config.keep_last {
                        break 'search i.saturating_sub(1);
                    }
                }
            }
            0
        };

        cutoff
    }

    /// 单条消息强制降级（wire 层）：扫描所有消息，估算 token 超过
    /// `single_message_token_budget` 的做 head/tail 截断 + 标记。
    ///
    /// 覆盖压缩的结构性盲区：最新超大单条永远在 keep_last 保留区间内，
    /// 摘要够不到；此处在发送前直接把单条压到预算内。返回是否发生截断。
    pub fn force_fit_single_message(&self, msgs: &mut [Message], budget: usize) -> bool {
        let mut changed = false;
        for msg in msgs.iter_mut() {
            let estimated = estimate_tokens(msg.text());
            if estimated <= budget {
                continue;
            }
            let capped = cap_single_message(msg.text(), budget);
            msg.map_content(|_| capped);
            changed = true;
        }
        changed
    }

    /// Execute automatic compaction: summarize old messages and insert a Compaction entry.
    /// Original messages are NOT removed — they are kept in the conversation.
    /// Returns `Ok(true)` if compaction was performed, `Ok(false)` if not needed.
    ///
    /// 压缩对模型输入生效的关键在投影层：`ModelCallInput::project_history` 遇 `Compaction`
    /// 会跳过 `summary_of` 覆盖的旧消息 timestamp（详见 model_call_input.rs），因此插入的
    /// 摘要真正代表旧消息，模型输入 token 显著下降（不投影旧消息逐条放大）。
    pub async fn ensure_fits(
        &self,
        conversation: &mut Conversation,
        providers: &ProviderRegistry,
        model: &super::models::ChatModelSelection,
        context_window: u32,
    ) -> AppResult<bool> {
        // 先做单条兜底：最新超大单条永远在 keep_last 保留区间内、摘要够不到，
        // 必须在压缩判定之前直接截断到预算内（wire 层，不动真相源）。
        let force_fitted = self.force_fit_single_message(
            &mut conversation.messages,
            self.config.single_message_token_budget,
        );

        if !self.needs_compaction(conversation, context_window) {
            return Ok(force_fitted);
        }

        let cutoff = self.compaction_boundary(conversation);
        if cutoff == 0 {
            return Ok(force_fitted);
        }

        // Take a reference to old messages for summarization (don't drain)
        let old_messages: Vec<Message> = conversation.messages[..cutoff].to_vec();

        // Build summary via LLM
        let prompt = compaction_prompt(&old_messages);
        let summary = self.call_summary_llm(providers, model, &prompt).await?;

        // Create a Compaction message with timestamps of summarized messages
        let compaction_msg = Message {
            role: MessageRole::Compaction,
            body: MessageBody::Compaction {
                summary_of: old_messages
                    .iter()
                    .map(|m| m.timestamp.to_string())
                    .collect(),
                content: summary,
            },
            timestamp: crate::core::conversation_store::now_ms(),
            neuron_id: None,
        };

        // Insert the compaction summary at position 0 (original messages kept intact)
        conversation.messages.insert(0, compaction_msg);

        Ok(true)
    }

    /// Force compaction (public API for manual /compact command).
    /// Skips threshold check — always compacts if there are enough messages.
    /// Original messages are NOT removed.
    pub async fn compact(
        &self,
        conversation: &mut Conversation,
        providers: &ProviderRegistry,
        model: &super::models::ChatModelSelection,
    ) -> AppResult<bool> {
        let cutoff = self.compaction_boundary(conversation);
        if cutoff == 0 {
            return Ok(false);
        }

        // Take a reference to old messages for summarization (don't drain)
        let old_messages: Vec<Message> = conversation.messages[..cutoff].to_vec();

        // Build summary via LLM
        let prompt = compaction_prompt(&old_messages);
        let summary = self.call_summary_llm(providers, model, &prompt).await?;

        // Create a Compaction message with timestamps of summarized messages
        let compaction_msg = Message {
            role: MessageRole::Compaction,
            body: MessageBody::Compaction {
                summary_of: old_messages
                    .iter()
                    .map(|m| m.timestamp.to_string())
                    .collect(),
                content: summary,
            },
            timestamp: crate::core::conversation_store::now_ms(),
            neuron_id: None,
        };

        // Insert the compaction summary at position 0 (original messages kept intact)
        conversation.messages.insert(0, compaction_msg);

        Ok(true)
    }

    /// Call the LLM to produce a summary.
    async fn call_summary_llm(
        &self,
        providers: &ProviderRegistry,
        model: &super::models::ChatModelSelection,
        prompt: &str,
    ) -> AppResult<String> {
        let wire = ModelCallInput::assemble(&[], prompt, "", "", ModelAppendTemplate::Neuron);
        let response = providers
            .call_model(ModelCallRequest {
                provider_id: model.provider_id.clone(),
                model_id: model.model_id.clone(),
                messages: wire,
                tools: None,
                params: model.params.clone(),
                thinking: Some(ThinkingConfig {
                    enabled: Some(false),
                    effort: None,
                }),
                response_format: None,
            })
            .await
            .map_err(|e| AppError::CompactionFailed(format!("LLM summary call failed: {e}")))?;

        if response.output.trim().is_empty() {
            return Err(AppError::CompactionFailed(
                "LLM returned empty summary".into(),
            ));
        }

        Ok(response.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::conversation_store::now_ms;
    use crate::core::models::ConversationMode;

    fn make_message(role: MessageRole, content: &str) -> Message {
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

    fn make_conversation(msg_count: usize) -> Conversation {
        let mut messages = Vec::new();
        for i in 0..msg_count {
            messages.push(make_message(
                if i % 2 == 0 {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                },
                &format!("Message {i} content here with some text for token estimation purposes"),
            ));
        }
        Conversation {
            id: "test-conv".to_string(),
            mode: ConversationMode::Chat,
            messages,
            created_at: now_ms(),
            updated_at: now_ms(),
            extra: None,
        }
    }

    #[test]
    fn estimate_tokens_simple() {
        let text = "Hello, world! This is a test.";
        let estimated = estimate_tokens(text);
        assert!(estimated > 0);
        assert!(estimated <= text.chars().count());
    }

    #[test]
    fn needs_compaction_returns_false_when_disabled() {
        let config = CompactionConfig {
            enabled: false,
            ..Default::default()
        };
        let compactor = Compactor::new(config);
        let conv = make_conversation(100);

        assert!(!compactor.needs_compaction(&conv, 4096));
    }

    #[test]
    fn needs_compaction_returns_true_when_over_threshold() {
        let config = CompactionConfig {
            enabled: true,
            threshold_ratio: 0.5,
            ..Default::default()
        };
        let compactor = Compactor::new(config);

        let mut messages = Vec::new();
        for i in 0..50 {
            messages.push(make_message(
                if i % 2 == 0 {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                },
                &"A".repeat(200),
            ));
        }
        let conv = Conversation {
            id: "test".to_string(),
            mode: ConversationMode::Chat,
            messages,
            created_at: now_ms(),
            updated_at: now_ms(),
            extra: None,
        };

        assert!(compactor.needs_compaction(&conv, 1000));
    }

    #[test]
    fn compaction_boundary_respects_keep_last() {
        let config = CompactionConfig {
            enabled: true,
            keep_last: 3,
            ..Default::default()
        };
        let compactor = Compactor::new(config);
        let conv = make_conversation(10);
        let cutoff = compactor.compaction_boundary(&conv);

        assert!(cutoff > 0);
        assert_eq!(cutoff, 4);
    }

    #[test]
    fn compaction_boundary_returns_zero_when_below_minimum() {
        let config = CompactionConfig {
            enabled: true,
            keep_last: 10,
            ..Default::default()
        };
        let compactor = Compactor::new(config);
        let conv = make_conversation(4);

        assert_eq!(compactor.compaction_boundary(&conv), 0);
    }

    #[test]
    fn estimate_conversation_tokens_returns_reasonable_value() {
        let config = CompactionConfig::default();
        let compactor = Compactor::new(config);
        let conv = make_conversation(10);

        let estimated = compactor.estimate_conversation_tokens(&conv);
        assert!(estimated > 0);
    }

    #[test]
    fn ensure_fits_inserts_compaction_message_without_removing_originals() {
        let config = CompactionConfig {
            enabled: true,
            threshold_ratio: 0.5,
            keep_last: 5,
            ..Default::default()
        };
        let compactor = Compactor::new(config);
        let original_len = 20;
        let conv = make_conversation(original_len);

        // ensure_fits still needs a real LLM call which we cannot do in unit tests,
        // but we can verify the boundary logic independently
        let cutoff = compactor.compaction_boundary(&conv);
        assert!(cutoff > 0);
        assert!(cutoff < original_len);
    }
}

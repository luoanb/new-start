//! Model call input helpers: immutable message-list surgery + assemble.
//!
//! Specs:
//! - `docs/specs/2026-08-02_model-call-input.md`
//! - `docs/specs/2026-08-02_19-29_model-call-input-call-sites.md`
//!
//! This type does not look up inserts or assemble a full `ModelCallRequest`.

use std::collections::HashSet;

use super::{
    error::{AppError, AppResult},
    models::{Message, MessageBody, MessageRole, ModelMessage, ModelMessageRole},
};

/// Built-in append templates, aligned with product intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelAppendTemplate {
    /// 神经元：角色/能力载体（是谁、怎么做）。`content` = 能力附文；`user_input` = 本轮任务。
    Neuron,
    /// 操作说明书：工具与输出契约（产出什么、何种格式）。`content` = insert 正文；`user_input` = 待处理输入。
    Manual,
}

/// Static tool for preparing model call message lists.
pub struct ModelCallInput;

impl ModelCallInput {
    /// Replace the first system prompt in `history`, or prepend one if absent.
    ///
    /// Does not mutate `history`; returns a new list.
    ///
    /// Empty `system_prompt`: never insert an empty System message (the LLM API
    /// validates non-empty content for non-tool messages). Instead, drop any
    /// leftover empty-content System messages while keeping non-empty ones
    /// (e.g. compaction summaries).
    pub fn replace_system(history: &[ModelMessage], system_prompt: &str) -> Vec<ModelMessage> {
        if system_prompt.trim().is_empty() {
            return history
                .iter()
                .filter(|m| !(m.role == ModelMessageRole::System && m.content.trim().is_empty()))
                .cloned()
                .collect();
        }
        let mut out = history.to_vec();
        let system = Self::message(ModelMessageRole::System, system_prompt);
        if let Some(idx) = out.iter().position(|m| m.role == ModelMessageRole::System) {
            out[idx] = system;
        } else {
            out.insert(0, system);
        }
        out
    }

    /// Append one message at the end of `history`.
    ///
    /// Does not mutate `history`; returns a new list.
    pub fn append(history: &[ModelMessage], message: ModelMessage) -> Vec<ModelMessage> {
        let mut out = history.to_vec();
        out.push(message);
        out
    }

    /// Insert `message` at `index`, discarding the original message at `index` and all after it.
    ///
    /// - `index == history.len()` appends with nothing discarded.
    /// - `index > history.len()` is an error.
    ///
    /// Does not mutate `history`; returns a new list.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidInput`] when `index` is out of range.
    pub fn insert_at(
        history: &[ModelMessage],
        index: usize,
        message: ModelMessage,
    ) -> AppResult<Vec<ModelMessage>> {
        if index > history.len() {
            return Err(AppError::InvalidInput(format!(
                "insert_at index {index} exceeds history length {}",
                history.len()
            )));
        }
        let mut out = history[..index].to_vec();
        out.push(message);
        Ok(out)
    }

    /// Join `content` and `user_input` using a built-in structured template.
    ///
    /// Design intent:
    /// - [`ModelAppendTemplate::Neuron`]: neuron copy = **role / capability** (who & how).
    ///   `content` is optional capability addendum; `user_input` is this turn's task.
    /// - [`ModelAppendTemplate::Manual`]: insert = **tool manual / output contract** (what & format).
    ///   `content` is the manual body; `user_input` is the payload to judge under that contract.
    ///
    /// Empty sides omit their sections (no blank headings).
    pub fn with_user_input_for_append(
        content: &str,
        user_input: &str,
        template: ModelAppendTemplate,
    ) -> String {
        match template {
            ModelAppendTemplate::Neuron => render_neuron_template(content, user_input),
            ModelAppendTemplate::Manual => render_manual_template(content, user_input),
        }
    }

    /// Assemble a full message list for `call_model`（独立组装点用，不参与主链路拼接）。
    ///
    /// - Empty `history`: `System(role_system)` + `User(body)`（与落库顺序一致：首轮 System + 输入）。
    /// - Non-empty `history`: `replace_system` with `role_system`, then append User(`body`).
    pub fn assemble(
        history: &[ModelMessage],
        role_system: &str,
        content: &str,
        user_input: &str,
        template: ModelAppendTemplate,
    ) -> Vec<ModelMessage> {
        let body = Self::with_user_input_for_append(content, user_input, template);
        if history.is_empty() {
            // 空历史：System(role_system) + User(body) 分开；role_system 空则无 System，
            // body 空则无 User（LLM API 校验非空 content）。
            let mut single = Vec::new();
            if !role_system.trim().is_empty() {
                single.push(Self::message(ModelMessageRole::System, role_system));
            }
            if !body.is_empty() {
                single.push(Self::message(ModelMessageRole::User, &body));
            }
            single
        } else {
            let mut messages = Self::replace_system(history, role_system);
            if !body.is_empty() {
                messages.push(Self::message(ModelMessageRole::User, &body));
            }
            messages
        }
    }

    /// Normalize tool-call/tool-result pairing before sending to the model.
    ///
    /// OpenAI-compatible providers (DeepSeek etc.) reject a message list where
    /// a `role=tool` message has no preceding assistant message declaring its
    /// `tool_call_id` ("tool must be a response to preceding tool_calls"), or an
    /// assistant message declares `tool_calls` that are never answered
    /// ("insufficient tool messages following tool_calls"). Broken pairs can
    /// appear in legacy/imported history or when only one of several parallel
    /// tool calls is executed. This pass self-heals the list:
    ///
    /// - Assistant tool_calls answered *in full* by `role=tool` messages are kept.
    /// - An *unanswered* assistant tool_calls message keeps its text but drops the
    ///   `tool_calls` field (degrades to a plain assistant message); if it has no
    ///   text at all the message is dropped entirely.
    /// - A `role=tool` message is dropped unless a *kept* assistant message
    ///   declares its `tool_call_id` (a degraded/dropped assistant invalidates
    ///   the tool messages answering it, otherwise the list ends with an orphan
    ///   tool message that providers reject).
    pub fn sanitize_tool_pairs(history: &[ModelMessage]) -> Vec<ModelMessage> {
        // 被 tool 消息应答的 tool_call_id：只有全部 call 都被应答的 assistant 才保留声明。
        let answered: HashSet<&str> = history
            .iter()
            .filter(|m| m.role == ModelMessageRole::Tool)
            .filter_map(|m| m.tool_call_id.as_deref())
            .collect();
        // 保留「完整」assistant 声明的 tool_call_id：这些 id 对应的 tool 消息才是合法的。
        let live: HashSet<&str> = history
            .iter()
            .filter(|m| {
                m.role == ModelMessageRole::Assistant
                    && m.tool_calls
                        .as_ref()
                        .map(|calls| calls.iter().all(|c| answered.contains(c.id.as_str())))
                        .unwrap_or(false)
            })
            .flat_map(|m| {
                m.tool_calls
                    .as_ref()
                    .into_iter()
                    .flat_map(|calls| calls.iter().map(|c| c.id.as_str()))
            })
            .collect();
        let mut out = Vec::with_capacity(history.len());
        for message in history {
            let orphan_call = message.role == ModelMessageRole::Assistant
                && message
                    .tool_calls
                    .as_ref()
                    .map(|calls| calls.iter().any(|c| !answered.contains(c.id.as_str())))
                    .unwrap_or(false);
            if orphan_call {
                if message.content.trim().is_empty() {
                    continue; // 纯 tool_calls、无文本 → 整条丢弃
                }
                out.push(ModelMessage {
                    tool_calls: None,
                    ..message.clone()
                });
                continue;
            }
            // 孤儿 tool 消息：无被保留的 assistant 声明其 id → 丢弃
            // （降级/丢弃 assistant 会连带使其 tool 结果失去前置 tool_calls）。
            if message.role == ModelMessageRole::Tool
                && !message
                    .tool_call_id
                    .as_deref()
                    .map(|id| live.contains(id))
                    .unwrap_or(false)
            {
                continue;
            }
            out.push(message.clone());
        }
        out
    }

    /// 落库 `Message[]` → 模型侧历史：`from_message` 逐条投影 + 防御过滤 + `sanitize_tool_pairs`。
    ///
    /// 真相源唯一约定：发送前（executor）与选型上下文（resolver）共用同一投影，不存在第二份
    /// 「给模型的 msg」。防御过滤丢弃「非 tool_call 且 content 空」的 assistant 残留（模型偶发
    /// 空响应，不清理会锁死后续调用）。
    pub fn project_history(messages: &[Message]) -> Vec<ModelMessage> {
        Self::sanitize_tool_pairs(
            &messages
                .iter()
                .filter_map(Self::from_message)
                .filter(|m| {
                    !(m.role == ModelMessageRole::Assistant
                        && m.tool_calls.as_ref().map_or(true, |c| c.is_empty())
                        && m.content.trim().is_empty())
                })
                .collect::<Vec<_>>(),
        )
    }

    /// `Message` → `ModelMessage` 投影（原 `call_service::message_to_model` 迁入）。
    ///
    /// - `Compaction` 摘要按 System 角色携带（与 engine 对齐），避免长会话压缩后丢失上下文。
    /// - `ToolResult` / `ToolCall` 按 tool / assistant 角色发送（OpenAI 兼容接口要求配对）。
    /// - `Nudge`（轮询简报）与 `RoleContext`（B2 角色声明）落库后均回灌为 User 文本：
    ///   落库顺序与 wire 注入顺序一致（首轮 System → RC → 输入 → 产物），回灌即还原模型
    ///   实际所见（历史 = wire，严格前缀累积）；条数由「生成一次落库一次」与每轮一次注入控制。
    pub fn from_message(message: &Message) -> Option<ModelMessage> {
        match &message.body {
            MessageBody::Compaction { content, .. } => Some(ModelMessage {
                role: ModelMessageRole::System,
                content: format!("[Previous conversation summary]: {content}"),
                tool_calls: None,
                tool_call_id: None,
            }),
            MessageBody::ToolResult {
                tool_call_id,
                content,
                ..
            } => Some(ModelMessage {
                role: ModelMessageRole::Tool,
                content: content.clone(),
                tool_calls: None,
                tool_call_id: Some(tool_call_id.clone()),
            }),
            MessageBody::ToolCall {
                content,
                tool_calls,
            } => Some(ModelMessage {
                role: ModelMessageRole::Assistant,
                content: content.clone(),
                tool_calls: Some(tool_calls.clone()),
                tool_call_id: None,
            }),
            MessageBody::Nudge { content } => Some(ModelMessage {
                role: ModelMessageRole::User,
                content: content.clone(),
                tool_calls: None,
                tool_call_id: None,
            }),
            MessageBody::RoleContext { content } => Some(ModelMessage {
                role: ModelMessageRole::User,
                content: content.clone(),
                tool_calls: None,
                tool_call_id: None,
            }),
            MessageBody::Text { content } => {
                let role = match message.role {
                    MessageRole::User => ModelMessageRole::User,
                    // Tool 角色不会携带 Text 正文（Tool 只对应 ToolResult），兜底按 Assistant 发送。
                    MessageRole::Assistant | MessageRole::Tool => ModelMessageRole::Assistant,
                    MessageRole::System => ModelMessageRole::System,
                    MessageRole::Compaction => unreachable!("handled above"),
                };
                Some(ModelMessage {
                    role,
                    content: content.clone(),
                    tool_calls: None,
                    tool_call_id: None,
                })
            }
        }
    }

    fn message(role: ModelMessageRole, content: &str) -> ModelMessage {
        ModelMessage {
            role,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

/// Neuron = 角色/能力载体：定义「是谁、怎么做」；本轮输入是要回应的任务。
fn render_neuron_template(content: &str, user_input: &str) -> String {
    if content.is_empty() && user_input.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = Vec::with_capacity(3);
    parts.push(
        "【神经元】角色与能力载体：按身份边界完成本轮任务；勿编造未提供的工具结果或事实。"
            .to_string(),
    );
    if !content.is_empty() {
        parts.push(format!("## 角色与能力\n\n{content}"));
    }
    if !user_input.is_empty() {
        parts.push(format!("## 本轮输入\n\n{user_input}"));
    }
    parts.join("\n\n")
}

/// Manual = 操作说明书：只约束工具职责与输出契约；待处理输入仅供事实/上下文。
fn render_manual_template(content: &str, user_input: &str) -> String {
    if content.is_empty() && user_input.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = Vec::with_capacity(3);
    parts.push(
        "【操作说明书】输出契约优先：严格按说明书规定的结构作答；待处理输入只提供事实与上下文，不得用散文替代规定格式。"
            .to_string(),
    );
    if !content.is_empty() {
        parts.push(format!("## 操作说明书（工具与输出契约）\n\n{content}"));
    }
    if !user_input.is_empty() {
        parts.push(format!("## 待处理输入\n\n{user_input}"));
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::super::models::ToolCall;
    use super::*;

    fn msg(role: ModelMessageRole, content: &str) -> ModelMessage {
        ModelMessage {
            role,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn tool_msg(content: &str, tool_call_id: &str) -> ModelMessage {
        ModelMessage {
            role: ModelMessageRole::Tool,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
        }
    }

    fn tool_call_msg(content: &str, ids: &[&str]) -> ModelMessage {
        ModelMessage {
            role: ModelMessageRole::Assistant,
            content: content.to_string(),
            tool_calls: Some(
                ids.iter()
                    .map(|id| ToolCall {
                        id: id.to_string(),
                        name: "test_tool".to_string(),
                        arguments: serde_json::json!({}),
                    })
                    .collect(),
            ),
            tool_call_id: None,
        }
    }

    #[test]
    fn replace_system_on_empty_history_prepends() {
        let out = ModelCallInput::replace_system(&[], "S");
        assert_eq!(out, vec![msg(ModelMessageRole::System, "S")]);
    }

    #[test]
    fn replace_system_when_no_system_prepends() {
        let history = vec![
            msg(ModelMessageRole::User, "u1"),
            msg(ModelMessageRole::Assistant, "a1"),
        ];
        let out = ModelCallInput::replace_system(&history, "S");
        assert_eq!(
            out,
            vec![
                msg(ModelMessageRole::System, "S"),
                msg(ModelMessageRole::User, "u1"),
                msg(ModelMessageRole::Assistant, "a1"),
            ]
        );
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn replace_system_replaces_first_system_only() {
        let history = vec![
            msg(ModelMessageRole::System, "old"),
            msg(ModelMessageRole::User, "u1"),
            msg(ModelMessageRole::System, "keep"),
        ];
        let out = ModelCallInput::replace_system(&history, "new");
        assert_eq!(
            out,
            vec![
                msg(ModelMessageRole::System, "new"),
                msg(ModelMessageRole::User, "u1"),
                msg(ModelMessageRole::System, "keep"),
            ]
        );
    }

    #[test]
    fn replace_system_with_empty_prompt_never_inserts_empty_system() {
        let history = vec![
            msg(ModelMessageRole::User, "u1"),
            msg(ModelMessageRole::Assistant, "a1"),
        ];
        let out = ModelCallInput::replace_system(&history, "");
        assert_eq!(out, history);
    }

    #[test]
    fn replace_system_with_empty_prompt_drops_empty_system_keeps_nonempty() {
        let history = vec![
            msg(ModelMessageRole::System, ""),
            msg(ModelMessageRole::User, "u1"),
            // 压缩摘要以 System 角色携带，非空时必须保留
            msg(
                ModelMessageRole::System,
                "[Previous conversation summary]: ...",
            ),
        ];
        let out = ModelCallInput::replace_system(&history, "");
        assert_eq!(
            out,
            vec![
                msg(ModelMessageRole::User, "u1"),
                msg(
                    ModelMessageRole::System,
                    "[Previous conversation summary]: ..."
                ),
            ]
        );
    }

    #[test]
    fn append_adds_at_end() {
        let history = vec![msg(ModelMessageRole::User, "u1")];
        let out = ModelCallInput::append(&history, msg(ModelMessageRole::User, "u2"));
        assert_eq!(
            out,
            vec![
                msg(ModelMessageRole::User, "u1"),
                msg(ModelMessageRole::User, "u2"),
            ]
        );
    }

    #[test]
    fn insert_at_truncates_tail() {
        let history = vec![
            msg(ModelMessageRole::System, "s"),
            msg(ModelMessageRole::User, "u1"),
            msg(ModelMessageRole::Assistant, "a1"),
        ];
        let out =
            ModelCallInput::insert_at(&history, 1, msg(ModelMessageRole::User, "cut")).unwrap();
        assert_eq!(
            out,
            vec![
                msg(ModelMessageRole::System, "s"),
                msg(ModelMessageRole::User, "cut"),
            ]
        );
    }

    #[test]
    fn insert_at_end_is_append() {
        let history = vec![msg(ModelMessageRole::User, "u1")];
        let out =
            ModelCallInput::insert_at(&history, 1, msg(ModelMessageRole::User, "u2")).unwrap();
        assert_eq!(
            out,
            vec![
                msg(ModelMessageRole::User, "u1"),
                msg(ModelMessageRole::User, "u2"),
            ]
        );
    }

    #[test]
    fn insert_at_out_of_range_errors() {
        let history = vec![msg(ModelMessageRole::User, "u1")];
        let err =
            ModelCallInput::insert_at(&history, 2, msg(ModelMessageRole::User, "x")).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[test]
    fn sanitize_keeps_paired_tool_messages() {
        let history = vec![
            msg(ModelMessageRole::User, "u1"),
            tool_call_msg("thinking", &["call_1"]),
            tool_msg("result ok", "call_1"),
            msg(ModelMessageRole::Assistant, "done"),
        ];
        let out = ModelCallInput::sanitize_tool_pairs(&history);
        assert_eq!(out, history);
    }

    #[test]
    fn sanitize_downgrades_orphan_with_text() {
        let history = vec![tool_call_msg("I will check", &["call_orphan"])];
        let out = ModelCallInput::sanitize_tool_pairs(&history);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, ModelMessageRole::Assistant);
        assert!(out[0].tool_calls.is_none());
        assert_eq!(out[0].content, "I will check");
    }

    #[test]
    fn sanitize_drops_orphan_without_text() {
        let history = vec![tool_call_msg("", &["call_orphan"])];
        let out = ModelCallInput::sanitize_tool_pairs(&history);
        assert!(out.is_empty());
    }

    #[test]
    fn sanitize_downgrades_partially_answered_calls_and_drops_orphan_tool() {
        // 并行 tool_calls 只执行首个（call_1 有结果、call_2 未应答）：assistant 降级为纯文本，
        // 其 tool 结果失去前置 tool_calls 声明 → 一并丢弃（否则 API 报 tool 无前置 tool_calls）。
        let history = vec![
            tool_call_msg("two calls", &["call_1", "call_2"]),
            tool_msg("only one answered", "call_1"),
        ];
        let out = ModelCallInput::sanitize_tool_pairs(&history);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, ModelMessageRole::Assistant);
        assert!(out[0].tool_calls.is_none());
        assert_eq!(out[0].content, "two calls");
    }

    #[test]
    fn sanitize_drops_tool_without_declaring_assistant() {
        // 孤儿 tool 消息：没有任何 assistant 声明其 id（历史导入损坏）→ 直接丢弃。
        let history = vec![
            msg(ModelMessageRole::User, "u1"),
            tool_msg("ghost result", "call_ghost"),
        ];
        let out = ModelCallInput::sanitize_tool_pairs(&history);
        assert_eq!(out, vec![msg(ModelMessageRole::User, "u1")]);
    }

    #[test]
    fn sanitize_drops_all_when_parallel_calls_unanswered() {
        // 纯 tool_calls 无文本且未应答 → assistant 丢弃；其 tool 结果也一并丢弃。
        let history = vec![
            tool_call_msg("", &["call_1", "call_2"]),
            tool_msg("result for first", "call_1"),
        ];
        let out = ModelCallInput::sanitize_tool_pairs(&history);
        assert!(out.is_empty());
    }

    #[test]
    fn with_user_input_neuron_structured_sections() {
        let out = ModelCallInput::with_user_input_for_append(
            "你是检索助手",
            "查一下天气",
            ModelAppendTemplate::Neuron,
        );
        assert!(out.contains("【神经元】"));
        assert!(out.contains("## 角色与能力\n\n你是检索助手"));
        assert!(out.contains("## 本轮输入\n\n查一下天气"));
    }

    #[test]
    fn with_user_input_manual_structured_sections() {
        let out = ModelCallInput::with_user_input_for_append(
            "只输出 JSON",
            "{\"a\":1}",
            ModelAppendTemplate::Manual,
        );
        assert!(out.contains("【操作说明书】"));
        assert!(out.contains("## 操作说明书（工具与输出契约）\n\n只输出 JSON"));
        assert!(out.contains("## 待处理输入\n\n{\"a\":1}"));
    }

    #[test]
    fn with_user_input_skips_empty_sides() {
        let neuron_only =
            ModelCallInput::with_user_input_for_append("hint", "", ModelAppendTemplate::Neuron);
        assert!(neuron_only.contains("## 角色与能力\n\nhint"));
        assert!(!neuron_only.contains("## 本轮输入"));

        let manual_only =
            ModelCallInput::with_user_input_for_append("", "hello", ModelAppendTemplate::Manual);
        assert!(manual_only.contains("## 待处理输入\n\nhello"));
        assert!(!manual_only.contains("## 操作说明书（工具与输出契约）"));

        assert_eq!(
            ModelCallInput::with_user_input_for_append("", "", ModelAppendTemplate::Neuron),
            ""
        );
    }

    #[test]
    fn assemble_empty_history_splits_system_and_user() {
        // 首轮：System(role_system) + User(body) 分开（与落库顺序一致，回灌即还原 wire）。
        let out = ModelCallInput::assemble(&[], "role", "c", "u", ModelAppendTemplate::Neuron);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].role, ModelMessageRole::System);
        assert_eq!(out[0].content, "role");
        assert_eq!(out[1].role, ModelMessageRole::User);
        assert!(out[1].content.starts_with("【神经元】"));
        assert!(out[1].content.contains("## 角色与能力\n\nc"));
        assert!(out[1].content.contains("## 本轮输入\n\nu"));
        // role_system 空（直连）：无 System，仅 User(body)。
        let out = ModelCallInput::assemble(&[], "", "c", "u", ModelAppendTemplate::Neuron);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, ModelMessageRole::User);
        // body 空：仅 System(role_system)。
        let out = ModelCallInput::assemble(&[], "role", "", "", ModelAppendTemplate::Neuron);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, ModelMessageRole::System);
    }

    #[test]
    fn assemble_nonempty_history_appends_user_body() {
        let history = vec![
            msg(ModelMessageRole::System, "old"),
            msg(ModelMessageRole::User, "u1"),
        ];
        let out = ModelCallInput::assemble(
            &history,
            "role",
            "manual",
            "payload",
            ModelAppendTemplate::Manual,
        );
        assert_eq!(out[0], msg(ModelMessageRole::System, "role"));
        assert_eq!(out[1], msg(ModelMessageRole::User, "u1"));
        assert_eq!(out[2].role, ModelMessageRole::User);
        assert!(out[2]
            .content
            .contains("## 操作说明书（工具与输出契约）\n\nmanual"));
        assert!(out[2].content.contains("## 待处理输入\n\npayload"));
    }

    #[test]
    fn from_message_refills_nudge_and_role_context() {
        let nudge = Message {
            role: MessageRole::User,
            body: MessageBody::Nudge {
                content: "brief".into(),
            },
            timestamp: 0,
            neuron_id: None,
        };
        let context = Message {
            role: MessageRole::User,
            body: MessageBody::RoleContext {
                content: "[当前角色]\nctx".into(),
            },
            timestamp: 0,
            neuron_id: None,
        };
        // 落库简报与角色声明均回灌为 User 文本（历史 = wire，严格前缀累积）。
        let nudge_back = ModelCallInput::from_message(&nudge).expect("nudge refills");
        assert_eq!(nudge_back.role, ModelMessageRole::User);
        assert_eq!(nudge_back.content, "brief");
        let ctx_back = ModelCallInput::from_message(&context).expect("role context refills");
        assert_eq!(ctx_back.role, ModelMessageRole::User);
        assert_eq!(ctx_back.content, "[当前角色]\nctx");
    }
}

//! Model call input helpers: immutable message-list surgery + assemble.
//!
//! Specs:
//! - `docs/specs/2026-08-02_model-call-input.md`
//! - `docs/specs/2026-08-02_19-29_model-call-input-call-sites.md`
//!
//! This type does not look up inserts or assemble a full `ModelCallRequest`.

use super::{
    error::{AppError, AppResult},
    models::{ModelMessage, ModelMessageRole},
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
    pub fn replace_system(history: &[ModelMessage], system_prompt: &str) -> Vec<ModelMessage> {
        let mut out = history.to_vec();
        let system = Self::message(ModelMessageRole::System, system_prompt);
        if let Some(idx) = out
            .iter()
            .position(|m| m.role == ModelMessageRole::System)
        {
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

    /// Assemble a full message list for `call_model`.
    ///
    /// - Empty `history`: fold `body` into System (no User message).
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
            let system = join_nonempty(role_system, &body);
            return Self::replace_system(&[], &system);
        }
        let with_system = Self::replace_system(history, role_system);
        if body.is_empty() {
            return with_system;
        }
        Self::append(
            &with_system,
            Self::message(ModelMessageRole::User, &body),
        )
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

fn join_nonempty(left: &str, right: &str) -> String {
    match (left.is_empty(), right.is_empty()) {
        (false, false) => format!("{left}\n\n{right}"),
        (false, true) => left.to_string(),
        (true, false) => right.to_string(),
        (true, true) => String::new(),
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
    use super::*;

    fn msg(role: ModelMessageRole, content: &str) -> ModelMessage {
        ModelMessage {
            role,
            content: content.to_string(),
            tool_calls: None,
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
        let err = ModelCallInput::insert_at(&history, 2, msg(ModelMessageRole::User, "x"))
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
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
        let neuron_only = ModelCallInput::with_user_input_for_append(
            "hint",
            "",
            ModelAppendTemplate::Neuron,
        );
        assert!(neuron_only.contains("## 角色与能力\n\nhint"));
        assert!(!neuron_only.contains("## 本轮输入"));

        let manual_only = ModelCallInput::with_user_input_for_append(
            "",
            "hello",
            ModelAppendTemplate::Manual,
        );
        assert!(manual_only.contains("## 待处理输入\n\nhello"));
        assert!(!manual_only.contains("## 操作说明书（工具与输出契约）"));

        assert_eq!(
            ModelCallInput::with_user_input_for_append("", "", ModelAppendTemplate::Neuron),
            ""
        );
    }

    #[test]
    fn assemble_empty_history_folds_body_into_system() {
        let out = ModelCallInput::assemble(
            &[],
            "role",
            "c",
            "u",
            ModelAppendTemplate::Neuron,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, ModelMessageRole::System);
        assert!(out[0].content.starts_with("role\n\n【神经元】"));
        assert!(out[0].content.contains("## 角色与能力\n\nc"));
        assert!(out[0].content.contains("## 本轮输入\n\nu"));
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
        assert!(out[2].content.contains("## 操作说明书（工具与输出契约）\n\nmanual"));
        assert!(out[2].content.contains("## 待处理输入\n\npayload"));
    }
}

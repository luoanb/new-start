//! Hook 概念收拢层：裁决 hook 的唯一静态清单（规则），与 `hook_judgements` 账本（数据库表）区分。
//!
//! - **HookDef 是「规则」**：`system_type` 标识、展示名 `label`、结构化输出契约 `response_format`、
//!   中性降级默认值 `neutral_fallback` —— 每个 hook 自带，就近定义。
//! - **HOOK_DEFS 是代码内静态清单**（非数据库表、不落盘、运行时只读）；新增裁决 hook =
//!   此处加一行 + 一个 hook 函数，`call_judgement` 与面板过滤下拉自动收拢。
//! - `SYSTEM_TYPE_SELECT_NEURON`（候选选择）非裁决 hook，不收拢；`SYSTEM_TYPE_*` 常量保留在
//!   `assistant_session.rs` 原位，此处引用。

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::core::{
    assistant_session::{
        SYSTEM_TYPE_COMPLETE_SCOPE, SYSTEM_TYPE_MATCH_TOPIC, SYSTEM_TYPE_REVISE_TOPIC,
        SYSTEM_TYPE_SCORE_FEEDBACK,
    },
    hook::defs::InjectPointId,
    openai_compat::ResponseFormatSpec,
};

/// 单个裁决 hook 的静态定义。
pub struct HookDef {
    /// system_type 标识（引用 assistant_session.rs 常量，常量保留原位）。
    pub system_type: &'static str,
    /// 展示名 i18n key（面板过滤下拉与记录展示的数据源）。
    pub label: &'static str,
    /// 挂载注入点（账本 `inject_point` 列来源；裁决均在 IP-1/IP-5 挂载）。
    pub inject_point: &'static str,
    /// hook 自带结构化输出契约（schema 就近定义；None = 无约束）。
    pub response_format: Option<ResponseFormatSpec>,
    /// 中性降级默认值（A 方案兜底语义：裁决失败时主轮次不中断）。
    pub neutral_fallback: fn() -> serde_json::Value,
}

/// hook 元信息（命令 `hook_defs_list` 出参；前端不感知 Rust 静态表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefMeta {
    pub system_type: String,
    pub label: String,
}

/// 裁决终态（三态；pending 是过程态，不入终态判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgementStatus {
    /// 一次成功（首轮即解析出合法 JSON 决策）。
    Ok,
    /// 首轮失败、带反馈重试 1 次后成功。
    RetriedOk,
    /// 重试后仍失败，使用 `neutral_fallback` 中性兜底。
    Downgraded,
}

impl JudgementStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JudgementStatus::Ok => "ok",
            JudgementStatus::RetriedOk => "retried_ok",
            JudgementStatus::Downgraded => "downgraded",
        }
    }
}

/// 单轮尝试明细（全量保留：原始输出全文 + 该轮解析失败原因）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptRecord {
    /// 轮次：1 = 首轮，2 = 重试轮。
    pub attempt: u32,
    /// 该轮模型原始输出（全文，不截断）。
    pub raw: String,
    /// 该轮解析失败原因（成功轮为 None）。
    pub error: Option<String>,
}

/// 裁决调用结果（`call_judgement` 返回契约）。
#[derive(Debug, Clone)]
pub struct JudgementOutcome {
    pub status: JudgementStatus,
    /// 成功 = 解析出的 JSON 决策；降级 = `def.neutral_fallback()`。
    pub decision: serde_json::Value,
    /// 最终轮模型原始输出。
    pub raw_response: String,
    /// 全量尝试明细（重试两轮原文均保留）。
    pub attempts_detail: Vec<AttemptRecord>,
    /// 失败/降级原因摘要。
    pub error: Option<String>,
    /// 总耗时（含重试），毫秒。
    pub duration_ms: u64,
}

/// 裁决调用锚点（落库定位：裁决卡挂载到哪个会话的哪条消息下方）。
#[derive(Debug, Clone)]
pub struct JudgementAnchor {
    pub conversation_id: String,
    /// 锚点消息索引；未绑定消息为 None。
    pub anchor_message_index: Option<i64>,
}

// ── JSON Schema（就近定义，每个 hook 一份，约束其决策输出）───────────────────

/// complete_scope：勾选已完成/已阻塞 scope 项。
pub const COMPLETE_SCOPE_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "completed_item_ids": { "type": "array", "items": { "type": "string" } },
    "blocked_item_ids": { "type": "array", "items": { "type": "string" } }
  },
  "required": ["completed_item_ids", "blocked_item_ids"],
  "additionalProperties": false
}"#;

/// match_topic：switch（切已有课题）/ create（新建）/ none（不创建不切换）。
/// strict 兼容：全字段 required，可选用 `["T","null"]` 联合表达。
pub const MATCH_TOPIC_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "action": { "type": "string", "enum": ["switch", "create", "none"] },
    "topic_id": { "type": ["string", "null"] },
    "name": { "type": ["string", "null"] },
    "description": { "type": ["string", "null"] },
    "scope_in": {
      "type": ["array", "null"],
      "items": {
        "type": "object",
        "properties": {
          "goal": { "type": "string" },
          "done_contract": { "type": "string" }
        },
        "required": ["goal", "done_contract"],
        "additionalProperties": false
      }
    }
  },
  "required": ["action", "topic_id", "name", "description", "scope_in"],
  "additionalProperties": false
}"#;

/// revise_topic：scope_in 增删改 + 修订原因。
pub const REVISE_TOPIC_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "reason": { "type": "string" },
    "add_items": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "goal": { "type": "string" },
          "done_contract": { "type": "string" }
        },
        "required": ["goal", "done_contract"],
        "additionalProperties": false
      }
    },
    "remove_item_ids": { "type": "array", "items": { "type": "string" } },
    "update_items": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "id": { "type": "string" },
          "goal": { "type": "string" },
          "done_contract": { "type": "string" }
        },
        "required": ["id"],
        "additionalProperties": false
      }
    }
  },
  "required": ["reason", "add_items", "remove_item_ids", "update_items"],
  "additionalProperties": false
}"#;

/// score_feedback：干预区间打分（-5..=5，非 0）。
pub const SCORE_FEEDBACK_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "score": { "type": "integer", "minimum": -5, "maximum": 5 }
  },
  "required": ["score"],
  "additionalProperties": false
}"#;

// ── 中性降级默认值（A 方案兜底语义，逐 hook 固化）────────────────────────────

fn fallback_complete_scope() -> serde_json::Value {
    serde_json::json!({ "completed_item_ids": [], "blocked_item_ids": [] })
}

fn fallback_match_topic() -> serde_json::Value {
    // none：不创建、不切换（hook 消费处显式三分支处理）。
    serde_json::json!({ "action": "none" })
}

fn fallback_revise_topic() -> serde_json::Value {
    // 空 diff：无任何 add/remove/update，不产生修订副作用。
    serde_json::json!({
        "reason": "",
        "add_items": [],
        "remove_item_ids": [],
        "update_items": []
    })
}

fn fallback_score_feedback() -> serde_json::Value {
    // 中性占位（消费处按终态 status 跳过，不应用打分）。
    serde_json::json!({ "score": 0 })
}

// ── 静态清单（新增裁决 hook = 此处加一行 + 一个 hook 函数）───────────────────

/// 全部裁决 hook 定义（运行时只读；面板过滤下拉由本表生成）。
pub static HOOK_DEFS: &[HookDef] = &[
    HookDef {
        system_type: SYSTEM_TYPE_COMPLETE_SCOPE,
        label: "hook.completeScope",
        inject_point: InjectPointId::AfterPersistOutcome.as_str(),
        response_format: Some(ResponseFormatSpec::JsonSchema {
            name: Cow::Borrowed("complete_scope"),
            schema: Cow::Borrowed(COMPLETE_SCOPE_SCHEMA),
        }),
        neutral_fallback: fallback_complete_scope,
    },
    HookDef {
        system_type: SYSTEM_TYPE_MATCH_TOPIC,
        label: "hook.matchTopic",
        inject_point: InjectPointId::AfterLoadContext.as_str(),
        response_format: Some(ResponseFormatSpec::JsonSchema {
            name: Cow::Borrowed("match_topic"),
            schema: Cow::Borrowed(MATCH_TOPIC_SCHEMA),
        }),
        neutral_fallback: fallback_match_topic,
    },
    HookDef {
        system_type: SYSTEM_TYPE_REVISE_TOPIC,
        label: "hook.reviseTopic",
        inject_point: InjectPointId::AfterPersistOutcome.as_str(),
        response_format: Some(ResponseFormatSpec::JsonSchema {
            name: Cow::Borrowed("revise_topic"),
            schema: Cow::Borrowed(REVISE_TOPIC_SCHEMA),
        }),
        neutral_fallback: fallback_revise_topic,
    },
    HookDef {
        system_type: SYSTEM_TYPE_SCORE_FEEDBACK,
        label: "hook.scoreFeedback",
        inject_point: InjectPointId::AfterLoadContext.as_str(),
        response_format: Some(ResponseFormatSpec::JsonSchema {
            name: Cow::Borrowed("score_feedback"),
            schema: Cow::Borrowed(SCORE_FEEDBACK_SCHEMA),
        }),
        neutral_fallback: fallback_score_feedback,
    },
];

/// 按 system_type 查找 hook 定义。
pub fn hook_def(system_type: &str) -> Option<&'static HookDef> {
    HOOK_DEFS.iter().find(|def| def.system_type == system_type)
}

/// hook 元信息列表（命令层出参）。
pub fn hook_defs_meta() -> Vec<HookDefMeta> {
    HOOK_DEFS
        .iter()
        .map(|def| HookDefMeta {
            system_type: def.system_type.to_string(),
            label: def.label.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_def_finds_all_four() {
        assert!(hook_def(SYSTEM_TYPE_COMPLETE_SCOPE).is_some());
        assert!(hook_def(SYSTEM_TYPE_MATCH_TOPIC).is_some());
        assert!(hook_def(SYSTEM_TYPE_REVISE_TOPIC).is_some());
        assert!(hook_def(SYSTEM_TYPE_SCORE_FEEDBACK).is_some());
        assert!(hook_def("assistant_select_neuron").is_none());
        assert!(hook_def("unknown").is_none());
    }

    #[test]
    fn fallback_values_are_neutral() {
        assert_eq!(
            (hook_def(SYSTEM_TYPE_COMPLETE_SCOPE).unwrap().neutral_fallback)(),
            serde_json::json!({ "completed_item_ids": [], "blocked_item_ids": [] })
        );
        assert_eq!(
            (hook_def(SYSTEM_TYPE_MATCH_TOPIC).unwrap().neutral_fallback)(),
            serde_json::json!({ "action": "none" })
        );
        let revise = (hook_def(SYSTEM_TYPE_REVISE_TOPIC).unwrap().neutral_fallback)();
        assert_eq!(revise["add_items"], serde_json::json!([]));
        assert_eq!(revise["remove_item_ids"], serde_json::json!([]));
        assert_eq!(revise["update_items"], serde_json::json!([]));
        assert_eq!(
            (hook_def(SYSTEM_TYPE_SCORE_FEEDBACK).unwrap().neutral_fallback)(),
            serde_json::json!({ "score": 0 })
        );
    }

    #[test]
    fn each_hook_carries_response_format_schema() {
        for def in HOOK_DEFS {
            assert!(
                matches!(def.response_format, Some(ResponseFormatSpec::JsonSchema { .. })),
                "{} should carry a json_schema",
                def.system_type
            );
        }
    }

    #[test]
    fn schemas_are_valid_strict_json_schema() {
        // strict 模式要求：可解析为对象、顶层含 additionalProperties: false。
        for def in HOOK_DEFS {
            let ResponseFormatSpec::JsonSchema { schema, .. } =
                def.response_format.as_ref().expect("hook carries schema")
            else {
                unreachable!()
            };
            let parsed = serde_json::from_str::<serde_json::Value>(schema.as_ref())
                .unwrap_or_else(|e| panic!("{} schema must parse: {e}", def.system_type));
            assert_eq!(
                parsed["additionalProperties"],
                serde_json::json!(false),
                "{} schema must declare additionalProperties:false",
                def.system_type
            );
        }
    }

    #[test]
    fn hook_defs_meta_maps_label() {
        let metas = hook_defs_meta();
        assert_eq!(metas.len(), 4);
        assert!(metas.iter().any(|m| m.system_type == SYSTEM_TYPE_COMPLETE_SCOPE
            && m.label == "hook.completeScope"));
    }

    #[test]
    fn judgement_status_serializes_snake_case() {
        assert_eq!(JudgementStatus::Ok.as_str(), "ok");
        assert_eq!(JudgementStatus::RetriedOk.as_str(), "retried_ok");
        assert_eq!(JudgementStatus::Downgraded.as_str(), "downgraded");
        let json = serde_json::to_string(&JudgementStatus::RetriedOk).unwrap();
        assert_eq!(json, r#""retried_ok""#);
    }
}

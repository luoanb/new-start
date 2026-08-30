//! Hook 概念收拢层：裁决 hook 的**共享类型**，与 `hook_judgements` 账本（数据库表）区分。
//!
//! - **HookDef 是「规则」**：`system_type` 标识、展示名 `label`、结构化输出契约 `response_format`、
//!   中性降级默认值 `neutral_fallback` —— 每个 hook 自带，就近定义在
//!   `hook/instances/<hook>.rs`，经 `registry::ACTIVE_HOOKS` 汇聚。
//! - **注册式管理**：新增/下线 hook = 实例清单增删（见 `registry`）；本模块只保留类型
//!   与查询入口，`hook_def()` / `hook_defs_meta()` 只查 `ACTIVE_HOOKS`
//!   （legacy system_type 在存量账本中按未知类型回退展示）。
//! - `SYSTEM_TYPE_SELECT_NEURON`（候选选择）非裁决 hook，不收拢；常量保留在
//!   `assistant_session.rs` 原位。

use serde::{Deserialize, Serialize};

use super::registry::ACTIVE_HOOKS;

/// 单个裁决 hook 的静态定义。
pub struct HookDef {
    /// system_type 标识（常量就近定义在 `hook/instances/<hook>.rs`）。
    pub system_type: &'static str,
    /// 展示名 i18n key（面板过滤下拉与记录展示的数据源）。
    pub label: &'static str,
    /// 挂载注入点（账本 `inject_point` 列来源；裁决均在 IP-1/IP-5 挂载）。
    pub inject_point: &'static str,
    /// hook 自带结构化输出契约（schema 就近定义；None = 无约束）。
    pub response_format: Option<crate::core::openai_compat::ResponseFormatSpec>,
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

/// 按 system_type 查启用 hook 定义（只查 `registry::ACTIVE_HOOKS`）。
pub fn hook_def(system_type: &str) -> Option<&'static HookDef> {
    ACTIVE_HOOKS
        .iter()
        .map(|h| &h.def)
        .find(|def| def.system_type == system_type)
}

/// 启用 hook 元信息列表（命令 `hook_defs_list` 出参）。
pub fn hook_defs_meta() -> Vec<HookDefMeta> {
    ACTIVE_HOOKS
        .iter()
        .map(|h| HookDefMeta {
            system_type: h.def.system_type.to_string(),
            label: h.def.label.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::hook::instances::{
        SYSTEM_TYPE_ROUND_REVIEW, SYSTEM_TYPE_USER_ROUND_JUDGEMENT,
    };

    #[test]
    fn hook_def_finds_merged_two() {
        assert!(hook_def(SYSTEM_TYPE_USER_ROUND_JUDGEMENT).is_some());
        assert!(hook_def(SYSTEM_TYPE_ROUND_REVIEW).is_some());
        // 旧四条休眠不注册（存量账本记录走面板未知类型回退）。
        assert!(hook_def("assistant_match_topic").is_none());
        assert!(hook_def("assistant_complete_scope").is_none());
        assert!(hook_def("assistant_score_feedback").is_none());
        assert!(hook_def("assistant_revise_topic").is_none());
        assert!(hook_def("assistant_select_neuron").is_none());
        assert!(hook_def("unknown").is_none());
    }

    #[test]
    fn fallback_values_are_neutral() {
        let user_round = (hook_def(SYSTEM_TYPE_USER_ROUND_JUDGEMENT)
            .unwrap()
            .neutral_fallback)();
        assert_eq!(user_round["score"], serde_json::json!(0));
        assert_eq!(user_round["action"], serde_json::json!("none"));
        assert_eq!(user_round["topic_id"], serde_json::Value::Null);

        let review = (hook_def(SYSTEM_TYPE_ROUND_REVIEW).unwrap().neutral_fallback)();
        assert_eq!(review["reason"], serde_json::json!(""));
        assert_eq!(review["add_items"], serde_json::json!([]));
        assert_eq!(review["remove_item_ids"], serde_json::json!([]));
        assert_eq!(review["update_items"], serde_json::json!([]));
        assert_eq!(review["completed_item_ids"], serde_json::json!([]));
        assert_eq!(review["blocked_item_ids"], serde_json::json!([]));
    }

    #[test]
    fn each_hook_carries_response_format_schema() {
        for h in ACTIVE_HOOKS {
            assert!(
                matches!(h.def.response_format, Some(crate::core::openai_compat::ResponseFormatSpec::JsonSchema { .. })),
                "{} should carry a json_schema",
                h.def.system_type
            );
        }
    }

    #[test]
    fn schemas_are_valid_strict_json_schema() {
        // strict 模式要求：可解析为对象、顶层含 additionalProperties: false。
        for h in ACTIVE_HOOKS {
            let crate::core::openai_compat::ResponseFormatSpec::JsonSchema { schema, .. } =
                h.def.response_format.as_ref().expect("hook carries schema")
            else {
                unreachable!()
            };
            let parsed = serde_json::from_str::<serde_json::Value>(schema.as_ref())
                .unwrap_or_else(|e| panic!("{} schema must parse: {e}", h.def.system_type));
            assert_eq!(
                parsed["additionalProperties"],
                serde_json::json!(false),
                "{} schema must declare additionalProperties:false",
                h.def.system_type
            );
        }
    }

    #[test]
    fn hook_defs_meta_maps_label() {
        let metas = hook_defs_meta();
        assert_eq!(metas.len(), 2);
        assert!(metas.iter().any(|m| m.system_type == SYSTEM_TYPE_ROUND_REVIEW
            && m.label == "hook.roundReview"));
        assert!(metas.iter().any(|m| m.system_type == SYSTEM_TYPE_USER_ROUND_JUDGEMENT
            && m.label == "hook.userRoundJudgement"));
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

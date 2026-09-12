//! 裁决定的**定义元数据**层：`JudgementSpec` + 定义清单 + 查询入口。
//!
//! 三阶段分离（定义 / 注册 / 开启，见 `core::hook::defs::HookRegistry`）：
//! - **定义**：`JudgementSpec` 就近定义在 `hook/instances/<hook>.rs`，经 `JUDGEMENT_SPECS`
//!   汇聚成本构建装配的裁决定义清单——**清单不代表注册，也不代表开启**。
//! - **注册 + 开启**：装配期把定义对应的核心 `HookDef` 注册进 `HookRegistry`，再显式开启。
//! - `SYSTEM_TYPE_SELECT_NEURON`（候选选择）非裁决，不收拢；常量保留在 `assistant_session.rs`。

use serde::{Deserialize, Serialize};

use super::instances;

/// 单个裁决的**定义元数据**（不含注册 / 开启状态）。
pub struct JudgementSpec {
    /// system_type 标识（常量就近定义在 `hook/instances/<hook>.rs`）。
    pub system_type: &'static str,
    /// 展示名 i18n key（面板过滤下拉与记录展示的数据源）。
    pub label: &'static str,
    /// 挂载注入点（账本 `inject_point` 列来源；裁决均在 IP-1/IP-5 挂载）。
    pub inject_point: &'static str,
    /// 自带结构化输出契约（schema 就近定义；None = 无约束）。
    pub response_format: Option<crate::core::models::ResponseFormatSpec>,
    /// 中性降级默认值（A 方案兜底语义：裁决失败时主轮次不中断）。
    pub neutral_fallback: fn() -> serde_json::Value,
}

/// 本构建装配的裁决定义清单（**定义**，不含注册 / 开启状态）。
///
/// 休眠的四条旧裁决（`score_feedback` / `match_topic` / `revise_topic` / `complete_scope`）
/// 源码保留为「定义·未注册」，不进本清单、不注册、不开启。
pub(crate) static JUDGEMENT_SPECS: &[&JudgementSpec] = &[
    &instances::user_round_judgement::SPEC,
    &instances::round_review::SPEC,
];

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

/// 按 system_type 查裁决定义。
pub fn judgement_spec(system_type: &str) -> Option<&'static JudgementSpec> {
    JUDGEMENT_SPECS
        .iter()
        .copied()
        .find(|spec| spec.system_type == system_type)
}

/// 裁决定义元信息列表（命令 `hook_defs_list` 出参）。
pub fn hook_defs_meta() -> Vec<HookDefMeta> {
    JUDGEMENT_SPECS
        .iter()
        .map(|spec| HookDefMeta {
            system_type: spec.system_type.to_string(),
            label: spec.label.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::hook::instances::{
        SYSTEM_TYPE_ROUND_REVIEW, SYSTEM_TYPE_USER_ROUND_JUDGEMENT,
    };

    #[test]
    fn judgement_spec_finds_wired_two() {
        assert!(judgement_spec(SYSTEM_TYPE_USER_ROUND_JUDGEMENT).is_some());
        assert!(judgement_spec(SYSTEM_TYPE_ROUND_REVIEW).is_some());
        // 旧四条休眠为「定义·未注册」，不进定义清单。
        assert!(judgement_spec("assistant_match_topic").is_none());
        assert!(judgement_spec("assistant_complete_scope").is_none());
        assert!(judgement_spec("assistant_score_feedback").is_none());
        assert!(judgement_spec("assistant_revise_topic").is_none());
        assert!(judgement_spec("assistant_select_neuron").is_none());
        assert!(judgement_spec("unknown").is_none());
    }

    #[test]
    fn fallback_values_are_neutral() {
        let user_round = (judgement_spec(SYSTEM_TYPE_USER_ROUND_JUDGEMENT)
            .unwrap()
            .neutral_fallback)();
        assert_eq!(user_round["score"], serde_json::json!(0));
        assert_eq!(user_round["action"], serde_json::json!("none"));
        assert_eq!(user_round["topic_id"], serde_json::Value::Null);

        let review = (judgement_spec(SYSTEM_TYPE_ROUND_REVIEW)
            .unwrap()
            .neutral_fallback)();
        assert_eq!(review["reason"], serde_json::json!(""));
        assert_eq!(review["add_items"], serde_json::json!([]));
        assert_eq!(review["remove_item_ids"], serde_json::json!([]));
        assert_eq!(review["update_items"], serde_json::json!([]));
        assert_eq!(review["completed_item_ids"], serde_json::json!([]));
        assert_eq!(review["blocked_item_ids"], serde_json::json!([]));
    }

    #[test]
    fn each_hook_carries_response_format_schema() {
        for spec in JUDGEMENT_SPECS {
            assert!(
                matches!(
                    spec.response_format,
                    Some(crate::core::models::ResponseFormatSpec::JsonSchema { .. })
                ),
                "{} should carry a json_schema",
                spec.system_type
            );
        }
    }

    #[test]
    fn schemas_are_valid_strict_json_schema() {
        // strict 模式要求：可解析为对象、顶层含 additionalProperties: false。
        for spec in JUDGEMENT_SPECS {
            let crate::core::models::ResponseFormatSpec::JsonSchema { schema, .. } =
                spec.response_format.as_ref().expect("hook carries schema")
            else {
                unreachable!()
            };
            let parsed = serde_json::from_str::<serde_json::Value>(schema.as_ref())
                .unwrap_or_else(|e| panic!("{} schema must parse: {e}", spec.system_type));
            assert_eq!(
                parsed["additionalProperties"],
                serde_json::json!(false),
                "{} schema must declare additionalProperties:false",
                spec.system_type
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

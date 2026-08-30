//! （休眠）IP-5 · 课题范围修订 hook：模型裁决 scope_in 增删改，逐项容错落库并留痕。
//!
//! 已被 [`super::round_review`]（合并复盘）取代，整体保留为休眠单元；
//! 回切 = 本实例移入 `registry::ACTIVE_HOOKS`。

use std::borrow::Cow;

use serde_json::{json, Value};

use crate::core::assistant_session::{append_revision_log, parse_scope_revision, AssistantHooks};
use crate::core::conversation_runner::{RoundContext, RoundTriggerKind};
use crate::core::error::{AppError, AppResult};
use crate::core::hook::defs::{BoxFuture, InjectPointId};
use crate::core::hook::judgement::{HookDef, JudgementAnchor};
use crate::core::hook::registry::{HookInstance, HookRun};
use crate::core::models::TopicStatus;
use crate::core::openai_compat::ResponseFormatSpec;
use crate::core::topic_store::now_ms;

pub const SYSTEM_TYPE_REVISE_TOPIC: &str = "assistant_revise_topic";

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

fn fallback_revise_topic() -> Value {
    // 空 diff：无任何 add/remove/update，不产生修订副作用。
    json!({
        "reason": "",
        "add_items": [],
        "remove_item_ids": [],
        "update_items": []
    })
}

pub(crate) const INSTANCE: HookInstance = HookInstance {
    def: HookDef {
        system_type: SYSTEM_TYPE_REVISE_TOPIC,
        label: "hook.reviseTopic",
        inject_point: InjectPointId::AfterPersistOutcome.as_str(),
        response_format: Some(ResponseFormatSpec::JsonSchema {
            name: Cow::Borrowed("revise_topic"),
            schema: Cow::Borrowed(REVISE_TOPIC_SCHEMA),
        }),
        neutral_fallback: fallback_revise_topic,
    },
    run: HookRun::After(run_boxed),
};

pub(crate) async fn run(hooks: &AssistantHooks<'_>, ctx: &RoundContext) -> AppResult<()> {
    let Some(topic_id) = ctx.topic_id.clone() else {
        tracing::info!(phase = "revise_topic_hook", "skip: no topic");
        return Ok(());
    };
    let topic = match hooks.assistant.topics()?.get(&topic_id)? {
        Some(topic) => topic,
        None => {
            tracing::info!(
                phase = "revise_topic_hook",
                topic_id = %topic_id,
                "skip: topic missing"
            );
            return Ok(());
        }
    };
    if topic.scope_in.is_empty() {
        tracing::info!(
            phase = "revise_topic_hook",
            topic_id = %topic_id,
            "skip: empty scope_in"
        );
        return Ok(());
    }
    // 暂停 / 等待用户课题不做变更写入（避免触发 mutate 报错）。
    if matches!(
        topic.status,
        TopicStatus::Paused | TopicStatus::WaitingUser
    ) {
        tracing::info!(
            phase = "revise_topic_hook",
            topic_id = %topic_id,
            status = ?topic.status,
            "skip: topic paused or waiting user"
        );
        return Ok(());
    }
    let outcome = ctx
        .outcome
        .as_ref()
        .ok_or_else(|| AppError::InvalidInput("revise_topic requires a finished round".into()))?;
    let model_output = outcome.model_output.clone();
    let tool_results = outcome.tool_results.clone();
    let trigger = match ctx.trigger {
        RoundTriggerKind::User => "user",
        RoundTriggerKind::ManualStep => "manual",
        RoundTriggerKind::Poller => "poller",
        RoundTriggerKind::AgentLoop => "agent_loop",
    };
    tracing::info!(
        phase = "revise_topic_hook",
        topic_id = %topic_id,
        trigger,
        scope_items = topic.scope_in.len(),
        "calling revise-topic model"
    );
    // 同源：用本轮主对话同一模型（用户所选），不读配置默认。
    let model = &ctx.model;
    let def = &INSTANCE.def;
    // after hook：用户消息已落库，锚点 = 触发轮用户消息在列表中的位置（本轮输入为末尾一条）。
    let anchor = JudgementAnchor {
        conversation_id: ctx.session_id.clone(),
        anchor_message_index: Some(ctx.messages.len().saturating_sub(1) as i64),
    };
    let outcome = hooks
        .assistant
        .call_judgement(
            def,
            anchor,
            json!({
                "topic_id": topic_id,
                "scope_in": topic.scope_in,
                "model_output": model_output,
                "tool_results": tool_results,
                "user_input": ctx.model_input,
                "trigger": trigger,
            }),
            &model,
            &ctx.messages,
        )
        .await?;
    let decision = &outcome.decision;
    let reason = decision
        .get("reason")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("（无 reason）")
        .to_string();
    // 当前各项状态快照：completed 门禁仅 User 轮放行（Poller/ManualStep 一律跳过）。
    let is_user_round = matches!(ctx.trigger, RoundTriggerKind::User);
    let mut status_of: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for item in &topic.scope_in {
        status_of.insert(item.id.as_str(), item.status.as_str());
    }
    let plan = parse_scope_revision(&decision, &status_of, is_user_round);
    let mut added = 0usize;
    let mut removed_ids: Vec<String> = Vec::new();
    let mut updated_ids: Vec<String> = Vec::new();
    let skipped_ids = plan.skipped_ids;
    {
        // 独立作用域：应用结束后释放 TopicStore 锁，供后续 append_revision_log 再取。
        let stores = hooks.assistant.topics()?;
        for (goal, contract) in &plan.add_items {
            match stores.add_scope_item(&topic_id, goal, contract) {
                Ok(_) => added += 1,
                Err(error) => tracing::warn!(
                    phase = "revise_topic_hook",
                    error = %error,
                    "add scope item failed"
                ),
            }
        }
        for item_id in &plan.remove_item_ids {
            match stores.delete_scope_item(&topic_id, item_id) {
                Ok(_) => removed_ids.push(item_id.clone()),
                Err(error) => tracing::warn!(
                    phase = "revise_topic_hook",
                    error = %error,
                    item_id,
                    "remove scope item failed"
                ),
            }
        }
        for (item_id, goal, contract) in &plan.update_items {
            match stores.update_scope_item(&topic_id, item_id, goal.as_deref(), contract.as_deref())
            {
                Ok(_) => updated_ids.push(item_id.clone()),
                Err(error) => tracing::warn!(
                    phase = "revise_topic_hook",
                    error = %error,
                    item_id,
                    "update scope item failed"
                ),
            }
        }
    }
    // 留痕：有实际应用（add/remove/update 任一）或门禁跳过时记录；空 diff 不写。
    if added > 0 || !removed_ids.is_empty() || !updated_ids.is_empty() || !skipped_ids.is_empty() {
        let removed_len = removed_ids.len();
        let updated_len = updated_ids.len();
        let skipped_len = skipped_ids.len();
        let event = json!({
            "ts": now_ms(),
            "trigger": trigger,
            "reason": reason,
            "added": added,
            "removed_ids": removed_ids,
            "updated_ids": updated_ids,
            "skipped_ids": skipped_ids,
        });
        let _ = append_revision_log(&hooks.assistant.topic_store, &topic_id, event);
        tracing::info!(
            phase = "revise_topic_hook",
            topic_id = %topic_id,
            trigger,
            added,
            removed = removed_len,
            updated = updated_len,
            skipped = skipped_len,
            "revision applied"
        );
    }
    Ok(())
}

fn run_boxed<'a>(
    hooks: &'a AssistantHooks<'a>,
    ctx: &'a RoundContext,
) -> BoxFuture<'a, AppResult<()>> {
    Box::pin(run(hooks, ctx))
}

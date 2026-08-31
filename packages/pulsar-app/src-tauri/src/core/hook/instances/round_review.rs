//! IP-5 AfterPersistOutcome · 轮次合并复盘：①scope 修订（add/remove/update）②验收勾选
//! （completed / blocked）。单次模型调用先修订后验收（2026-08-30 spec 合并自
//! revise_topic + complete_scope）。
//!
//! 门控下沉在 run 内：仅收尾轮触发（`is_settling_round`：无工具声明且无工具执行），
//! 工具轮中间产物不做裁决；暂停 / 等待用户课题不写入。

use std::borrow::Cow;
use std::collections::HashMap;

use serde_json::{json, Value};

use crate::core::assistant_session::{
    append_revision_log, is_settling_round, parse_scope_revision, should_delay_close,
    AssistantHooks,
};
use crate::core::conversation_runner::{RoundContext, RoundTriggerKind};
use crate::core::error::AppResult;
use crate::core::hook::defs::{BoxFuture, InjectPointId};
use crate::core::hook::judgement::{HookDef, JudgementAnchor};
use crate::core::hook::registry::{HookInstance, HookRun};
use crate::core::log_phase::PHASE_HOOK_ROUND_REVIEW;
use crate::core::models::TopicStatus;
use crate::core::openai_compat::ResponseFormatSpec;
use crate::core::topic_store::now_ms;

pub const SYSTEM_TYPE_ROUND_REVIEW: &str = "assistant_round_review";

pub const ROUND_REVIEW_SCHEMA: &str = r#"{
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
    },
    "completed_item_ids": { "type": "array", "items": { "type": "string" } },
    "blocked_item_ids": { "type": "array", "items": { "type": "string" } },
    "blocked_reasons": { "type": "object", "additionalProperties": { "type": "string" } }
  },
  "required": ["reason", "add_items", "remove_item_ids", "update_items",
               "completed_item_ids", "blocked_item_ids", "blocked_reasons"],
  "additionalProperties": false
}"#;

fn fallback_round_review() -> Value {
    // 空 diff + 空勾选：无任何修订与验收副作用。
    json!({
        "reason": "",
        "add_items": [],
        "remove_item_ids": [],
        "update_items": [],
        "completed_item_ids": [],
        "blocked_item_ids": [],
        "blocked_reasons": {}
    })
}

pub(crate) const INSTANCE: HookInstance = HookInstance {
    def: HookDef {
        system_type: SYSTEM_TYPE_ROUND_REVIEW,
        label: "hook.roundReview",
        inject_point: InjectPointId::AfterPersistOutcome.as_str(),
        response_format: Some(ResponseFormatSpec::JsonSchema {
            name: Cow::Borrowed("round_review"),
            schema: Cow::Borrowed(ROUND_REVIEW_SCHEMA),
        }),
        neutral_fallback: fallback_round_review,
    },
    run: HookRun::After(run_boxed),
};

pub(crate) async fn run(hooks: &AssistantHooks<'_>, ctx: &RoundContext) -> AppResult<()> {
    let Some(topic_id) = ctx.topic_id.clone() else {
        tracing::info!(phase = PHASE_HOOK_ROUND_REVIEW, "skip: no topic");
        return Ok(());
    };
    let topic = match hooks.assistant.topics()?.get(&topic_id)? {
        Some(topic) => topic,
        None => {
            tracing::info!(
                phase = PHASE_HOOK_ROUND_REVIEW,
                topic_id = %topic_id,
                "skip: topic missing"
            );
            return Ok(());
        }
    };
    // 暂停 / 等待用户课题不做裁决写入（避免触发 mutate 报错）。
    if matches!(
        topic.status,
        TopicStatus::Paused | TopicStatus::WaitingUser
    ) {
        tracing::info!(
            phase = PHASE_HOOK_ROUND_REVIEW,
            topic_id = %topic_id,
            status = ?topic.status,
            "skip: topic paused or waiting user"
        );
        return Ok(());
    }
    // 空待办收尾：scope_in 为空时无任何可推进事项 → 置 Done，避免 Poller 每轮空转调模型
    //（空 scope 可能来自轮次复盘删光全部项或 legacy 迁移数据；derive_topic_state(&[])
    //  恒推导为 Todo，不主动收尾课题将永远被轮询推进）。
    if topic.scope_in.is_empty() {
        hooks
            .assistant
            .topics()?
            .set_status(&topic_id, TopicStatus::Done)?;
        tracing::info!(
            phase = PHASE_HOOK_ROUND_REVIEW,
            topic_id = %topic_id,
            "empty scope_in; topic closed as done"
        );
        return Ok(());
    }
    // 本轮最后一条是否为工具调用结果（persist_outcome 先于 after hooks，反映本轮）。
    let last_is_tool = hooks
        .assistant
        .runner
        .last_message_is_tool_result(&ctx.session_id)?;
    // 收尾关闭判断（前置）：WrappingUp 课题在本轮以文本收尾（无工具调用）后关闭。
    if topic.status == TopicStatus::WrappingUp {
        if !last_is_tool {
            hooks
                .assistant
                .topics()?
                .set_status(&topic_id, TopicStatus::Done)?;
            tracing::info!(
                phase = PHASE_HOOK_ROUND_REVIEW,
                topic_id = %topic_id,
                "wrap-up round finished; topic closed"
            );
        }
        return Ok(());
    }
    // 收尾轮门控：工具轮（声明或执行任一存在）的中间产物不触发裁决。
    let Some(outcome) = ctx.outcome.as_ref().filter(|o| is_settling_round(o)) else {
        tracing::info!(
            phase = PHASE_HOOK_ROUND_REVIEW,
            tool_calls = ctx.outcome.as_ref().map_or(0, |o| o.tool_calls.as_ref().map_or(0, Vec::len)),
            tool_results = ctx.outcome.as_ref().map_or(0, |o| o.tool_results.len()),
            "skip: not a settling round"
        );
        return Ok(());
    };
    let model_output = outcome.model_output.clone();
    let tool_results = outcome.tool_results.clone();
    let trigger = match ctx.trigger {
        RoundTriggerKind::User => "user",
        RoundTriggerKind::ManualStep => "manual",
        RoundTriggerKind::Poller => "poller",
        RoundTriggerKind::AgentLoop => "agent_loop",
    };
    tracing::info!(
        phase = PHASE_HOOK_ROUND_REVIEW,
        topic_id = %topic_id,
        trigger,
        scope_items = topic.scope_in.len(),
        "calling round-review model"
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
    // ① 修订消费（原 revise_topic 逻辑）：先改内容，新加项本轮即可参与验收。
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
        // 独立作用域：应用结束后释放 TopicStore 锁，供后续写库与留痕再取。
        let stores = hooks.assistant.topics()?;
        for (goal, contract) in &plan.add_items {
            match stores.add_scope_item(&topic_id, goal, contract) {
                Ok(_) => added += 1,
                Err(error) => tracing::warn!(
                    phase = PHASE_HOOK_ROUND_REVIEW,
                    error = %error,
                    "add scope item failed"
                ),
            }
        }
        for item_id in &plan.remove_item_ids {
            match stores.delete_scope_item(&topic_id, item_id) {
                Ok(_) => removed_ids.push(item_id.clone()),
                Err(error) => tracing::warn!(
                    phase = PHASE_HOOK_ROUND_REVIEW,
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
                    phase = PHASE_HOOK_ROUND_REVIEW,
                    error = %error,
                    item_id,
                    "update scope item failed"
                ),
            }
        }
    }
    // ② 验收消费（原 complete_scope 逻辑）：勾选完成 / 阻塞项。
    let completed_ids: Vec<String> = decision
        .get("completed_item_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let blocked_ids: Vec<String> = decision
        .get("blocked_item_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for item_id in &completed_ids {
        let _ = hooks
            .assistant
            .topics()?
            .complete_scope_item(&topic_id, item_id);
    }
    let blocked_reasons: HashMap<String, String> = decision
        .get("blocked_reasons")
        .and_then(|v| v.as_object())
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().trim().to_string()))
                .collect()
        })
        .unwrap_or_default();
    for item_id in &blocked_ids {
        let _ = hooks.assistant.topics()?.mark_scope_item_blocked(
            &topic_id,
            item_id,
            blocked_reasons.get(item_id).map(|r| r.as_str()),
        );
    }
    tracing::info!(
        phase = PHASE_HOOK_ROUND_REVIEW,
        topic_id = %topic_id,
        trigger,
        added,
        removed = removed_ids.len(),
        updated = updated_ids.len(),
        skipped = skipped_ids.len(),
        completed = completed_ids.len(),
        blocked = blocked_ids.len(),
        "round review applied"
    );
    // 合并留痕：修订或验收任一有实际应用时记一条事件（两部分摘要）；全空不写。
    if added > 0
        || !removed_ids.is_empty()
        || !updated_ids.is_empty()
        || !skipped_ids.is_empty()
        || !completed_ids.is_empty()
        || !blocked_ids.is_empty()
    {
        let event = json!({
            "ts": now_ms(),
            "trigger": trigger,
            "reason": reason,
            "added": added,
            "removed_ids": removed_ids,
            "updated_ids": updated_ids,
            "skipped_ids": skipped_ids,
            "completed_ids": completed_ids,
            "blocked_ids": blocked_ids,
        });
        let _ = append_revision_log(&hooks.assistant.topic_store, &topic_id, event);
    }
    // 延迟关闭判断（后置）：最后一项本轮完成，但本轮以工具调用结束（模型尚未产出
    // 最终总结）→ 置 WrappingUp 保持轮询，下一轮给收尾机会，而不是直接关闭课题。
    let topic_after = match hooks.assistant.topics()?.get(&topic_id)? {
        Some(topic) => topic,
        None => return Ok(()),
    };
    if should_delay_close(&topic_after.status, last_is_tool) {
        hooks
            .assistant
            .topics()?
            .set_status(&topic_id, TopicStatus::WrappingUp)?;
        tracing::info!(
            phase = PHASE_HOOK_ROUND_REVIEW,
            topic_id = %topic_id,
            "scope completed via tool round; topic wrapping up"
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

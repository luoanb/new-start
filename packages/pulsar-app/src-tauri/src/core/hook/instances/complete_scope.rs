//! （休眠）IP-5 · 进度验收 hook：模型裁决 scope 项勾选完成 / 阻塞，逐项容错落库。
//!
//! 已被 [`super::round_review`]（合并复盘）取代，整体保留为休眠单元；
//! 回切 = 本实例移入 `registry::ACTIVE_HOOKS`。

use std::borrow::Cow;

use serde_json::{json, Value};

use crate::core::assistant_session::{should_delay_close, AssistantHooks};
use crate::core::conversation_runner::RoundContext;
use crate::core::error::{AppError, AppResult};
use crate::core::hook::defs::{BoxFuture, InjectPointId};
use crate::core::hook::judgement::{HookDef, JudgementAnchor};
use crate::core::hook::registry::{HookInstance, HookRun};
use crate::core::models::TopicStatus;
use crate::core::openai_compat::ResponseFormatSpec;

pub const SYSTEM_TYPE_COMPLETE_SCOPE: &str = "assistant_complete_scope";

pub const COMPLETE_SCOPE_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "completed_item_ids": { "type": "array", "items": { "type": "string" } },
    "blocked_item_ids": { "type": "array", "items": { "type": "string" } }
  },
  "required": ["completed_item_ids", "blocked_item_ids"],
  "additionalProperties": false
}"#;

fn fallback_complete_scope() -> Value {
    json!({ "completed_item_ids": [], "blocked_item_ids": [] })
}

pub(crate) const INSTANCE: HookInstance = HookInstance {
    def: HookDef {
        system_type: SYSTEM_TYPE_COMPLETE_SCOPE,
        label: "hook.completeScope",
        inject_point: InjectPointId::AfterPersistOutcome.as_str(),
        response_format: Some(ResponseFormatSpec::JsonSchema {
            name: Cow::Borrowed("complete_scope"),
            schema: Cow::Borrowed(COMPLETE_SCOPE_SCHEMA),
        }),
        neutral_fallback: fallback_complete_scope,
    },
    run: HookRun::After(run_boxed),
};

pub(crate) async fn run(hooks: &AssistantHooks<'_>, ctx: &RoundContext) -> AppResult<()> {
    let Some(topic_id) = ctx.topic_id.clone() else {
        tracing::info!(phase = "complete_scope_hook", "skip: no topic");
        return Ok(());
    };
    let topic = match hooks.assistant.topics()?.get(&topic_id)? {
        Some(topic) => topic,
        None => {
            tracing::info!(phase = "complete_scope_hook", topic_id = %topic_id, "skip: topic missing");
            return Ok(());
        }
    };
    // 暂停 / 等待用户课题不做裁决写入（避免触发 mutate 报错）。
    if matches!(
        topic.status,
        TopicStatus::Paused | TopicStatus::WaitingUser
    ) {
        tracing::info!(
            phase = "complete_scope_hook",
            topic_id = %topic_id,
            status = ?topic.status,
            "skip: topic paused or waiting user"
        );
        return Ok(());
    }
    // 空待办收尾：scope_in 为空时无任何可推进事项 → 置 Done，避免 Poller 每轮空转调模型
    //（空 scope 可能来自 revise_topic 删光全部项或 legacy 迁移数据；derive_topic_state(&[])
    //  恒推导为 Todo，不主动收尾课题将永远被轮询推进）。
    if topic.scope_in.is_empty() {
        hooks
            .assistant
            .topics()?
            .set_status(&topic_id, TopicStatus::Done)?;
        tracing::info!(
            phase = "complete_scope_hook",
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
                phase = "complete_scope_hook",
                topic_id = %topic_id,
                "wrap-up round finished; topic closed"
            );
        }
        return Ok(());
    }
    let outcome = ctx
        .outcome
        .as_ref()
        .ok_or_else(|| AppError::InvalidInput("complete_scope requires a finished round".into()))?;
    let model_output = outcome.model_output.clone();
    let tool_results = outcome.tool_results.clone();
    tracing::info!(
        phase = "complete_scope_hook",
        topic_id = %topic_id,
        scope_items = topic.scope_in.len(),
        "calling complete-scope model"
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
            }),
            &model,
            &ctx.messages,
        )
        .await?;
    let decision = &outcome.decision;
    let ids = decision
        .get("completed_item_ids")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let blocked_ids = decision
        .get("blocked_item_ids")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    tracing::info!(
        phase = "complete_scope_hook",
        completed = ids.len(),
        blocked = blocked_ids.len(),
        "updating scope items"
    );
    for id in &ids {
        let Some(item_id) = id.as_str() else {
            continue;
        };
        let _ = hooks
            .assistant
            .topics()?
            .complete_scope_item(&topic_id, item_id);
    }
    for id in &blocked_ids {
        let Some(item_id) = id.as_str() else {
            continue;
        };
        let _ = hooks
            .assistant
            .topics()?
            .mark_scope_item_blocked(&topic_id, item_id);
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
            phase = "complete_scope_hook",
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

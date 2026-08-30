//! （休眠）IP-1 · 课题匹配 hook：模型裁决 action（switch → 切已有课题；create → 新建）。
//!
//! 已被 [`super::user_round_judgement`]（合并裁决）取代，整体保留为休眠单元；
//! 回切 = 本实例移入 `registry::ACTIVE_HOOKS`。

use std::borrow::Cow;

use serde_json::{json, Value};

use crate::core::assistant_session::{emergency_scope_in, AssistantHooks};
use crate::core::conversation_runner::RoundContext;
use crate::core::error::{AppError, AppResult};
use crate::core::hook::defs::{BoxFuture, InjectPointId};
use crate::core::hook::judgement::{HookDef, JudgementAnchor};
use crate::core::hook::registry::{HookInstance, HookRun};
use crate::core::log_phase::PHASE_HOOK_MATCH_TOPIC;
use crate::core::openai_compat::ResponseFormatSpec;

pub const SYSTEM_TYPE_MATCH_TOPIC: &str = "assistant_match_topic";

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

fn fallback_match_topic() -> Value {
    // none：不创建、不切换（hook 消费处显式三分支处理）。
    json!({ "action": "none" })
}

pub(crate) const INSTANCE: HookInstance = HookInstance {
    def: HookDef {
        system_type: SYSTEM_TYPE_MATCH_TOPIC,
        label: "hook.matchTopic",
        inject_point: InjectPointId::AfterLoadContext.as_str(),
        response_format: Some(ResponseFormatSpec::JsonSchema {
            name: Cow::Borrowed("match_topic"),
            schema: Cow::Borrowed(MATCH_TOPIC_SCHEMA),
        }),
        neutral_fallback: fallback_match_topic,
    },
    run: HookRun::Before(run_boxed),
};

pub(crate) async fn run(hooks: &AssistantHooks<'_>, ctx: &mut RoundContext) -> AppResult<()> {
    // 同源：用本轮主对话同一模型（用户所选），不读配置默认。
    let model = &ctx.model;
    let unfinished = hooks.assistant.topics()?.list_unfinished()?;
    tracing::info!(
        phase = PHASE_HOOK_MATCH_TOPIC,
        unfinished = unfinished.len(),
        session_id = %ctx.session_id,
        "calling match-topic model"
    );
    let def = &INSTANCE.def;
    // before hook：用户消息尚未落库，锚点 = 当前消息列表末尾（用户消息即将落库的位置）。
    let anchor = JudgementAnchor {
        conversation_id: ctx.session_id.clone(),
        anchor_message_index: Some(ctx.messages.len() as i64),
    };
    let outcome = hooks
        .assistant
        .call_judgement(
            def,
            anchor,
            json!({
                "user_input": ctx.model_input,
                "current_session_id": ctx.session_id,
                "topics": unfinished.iter().map(|t| json!({
                    "id": t.id,
                    "name": t.name,
                    "description": t.description,
                    "status": t.status,
                    "session_id": t.session_id,
                    "progress": t.progress,
                    "scope_in": t.scope_in,
                })).collect::<Vec<_>>(),
            }),
            &model,
            &ctx.messages,
        )
        .await?;
    let decision = &outcome.decision;

    let action = decision
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("create");
    tracing::info!(phase = PHASE_HOOK_MATCH_TOPIC, action, "match decision");
    match action {
        "switch" => {
            let topic_id = decision
                .get("topic_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AppError::InvalidInput("match topic switch missing topic_id".into())
                })?;
            let topic = match hooks.assistant.topics()?.get(topic_id)? {
                Some(topic) => topic,
                None => {
                    let created = hooks
                        .create_bound_topic_from_decision(ctx, decision, true)
                        .or_else(|error| {
                            tracing::warn!(
                                phase = PHASE_HOOK_MATCH_TOPIC,
                                error = %error,
                                "switch missing and decision lacked scope_in; using emergency scope"
                            );
                            hooks.create_bound_topic_with_scope(
                                ctx,
                                None,
                                None,
                                emergency_scope_in(ctx),
                            )
                        })?;
                    tracing::warn!(
                        phase = PHASE_HOOK_MATCH_TOPIC,
                        requested_topic_id = topic_id,
                        created_topic_id = %created.id,
                        "switch target missing; created topic"
                    );
                    ctx.topic_id = Some(created.id);
                    return Ok(());
                }
            };
            if let Some(bound_session) = topic.session_id.clone() {
                if bound_session != ctx.session_id {
                    // 切换到目标课题绑定的会话：runner 检测到 session_id 变化后自动 reload。
                    tracing::info!(
                        phase = PHASE_HOOK_MATCH_TOPIC,
                        from_session = %ctx.session_id,
                        to_session = %bound_session,
                        topic_id = %topic.id,
                        "switching session"
                    );
                    ctx.session_id = bound_session;
                    ctx.topic_id = Some(topic.id);
                } else {
                    ctx.topic_id = Some(topic.id);
                }
            } else {
                let bound = hooks
                    .assistant
                    .topics()?
                    .bind_session(&topic.id, &ctx.session_id)?;
                ctx.topic_id = Some(bound.id);
            }
        }
        "none" => {
            // 中性语义（A 降级兜底 action=none）：不创建、不切换，保持当前绑定。
            tracing::info!(phase = PHASE_HOOK_MATCH_TOPIC, "match decision: none (no-op)");
        }
        _ => {
            if ctx.topic_id.is_none() {
                let created = hooks.create_bound_topic_from_decision(ctx, decision, false)?;
                tracing::info!(
                    phase = PHASE_HOOK_MATCH_TOPIC,
                    topic_id = %created.id,
                    scope_items = created.scope_in.len(),
                    "created bound topic with scope_in"
                );
                ctx.topic_id = Some(created.id);
            }
        }
    }
    Ok(())
}

fn run_boxed<'a>(
    hooks: &'a AssistantHooks<'a>,
    ctx: &'a mut RoundContext,
) -> BoxFuture<'a, AppResult<()>> {
    Box::pin(run(hooks, ctx))
}

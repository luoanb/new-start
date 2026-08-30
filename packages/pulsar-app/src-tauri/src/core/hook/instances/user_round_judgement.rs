//! IP-1 AfterLoadContext · 用户轮合并裁决：①介入打分 ②课题路由（switch / create / none）。
//!
//! 单次模型调用同时承担两项职责（2026-08-30 spec 合并自 score_feedback + match_topic）。
//! 门控下沉在 run 内：未绑定课题（含首轮）必跑；已绑定课题每 3 条用户消息复核一次。

use std::borrow::Cow;

use serde_json::{json, Value};

use crate::core::assistant_session::{
    emergency_scope_in, interval_neuron_ids, need_user_round_judgement, read_assistant_state,
    AssistantHooks,
};
use crate::core::conversation_runner::RoundContext;
use crate::core::error::AppResult;
use crate::core::hook::defs::{BoxFuture, InjectPointId};
use crate::core::hook::judgement::{HookDef, JudgementAnchor};
use crate::core::hook::registry::{HookInstance, HookRun};
use crate::core::openai_compat::ResponseFormatSpec;

pub const SYSTEM_TYPE_USER_ROUND_JUDGEMENT: &str = "assistant_user_round_judgement";

/// strict 兼容：全字段 required，可选用 `["T","null"]` 联合表达。
pub const USER_ROUND_JUDGEMENT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "score": { "type": "integer" },
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
  "required": ["score", "action", "topic_id", "name", "description", "scope_in"],
  "additionalProperties": false
}"#;

fn fallback_user_round_judgement() -> Value {
    // score=0 → 消费处视为「不打分」；action=none → 不创建、不切换。
    json!({
        "score": 0,
        "action": "none",
        "topic_id": null,
        "name": null,
        "description": null,
        "scope_in": null
    })
}

pub(crate) const INSTANCE: HookInstance = HookInstance {
    def: HookDef {
        system_type: SYSTEM_TYPE_USER_ROUND_JUDGEMENT,
        label: "hook.userRoundJudgement",
        inject_point: InjectPointId::AfterLoadContext.as_str(),
        response_format: Some(ResponseFormatSpec::JsonSchema {
            name: Cow::Borrowed("user_round_judgement"),
            schema: Cow::Borrowed(USER_ROUND_JUDGEMENT_SCHEMA),
        }),
        neutral_fallback: fallback_user_round_judgement,
    },
    run: HookRun::Before(run_boxed),
};

pub(crate) async fn run(hooks: &AssistantHooks<'_>, ctx: &mut RoundContext) -> AppResult<()> {
    // 门控：未绑定课题（含首轮 user_rounds==0）必跑；已绑定按 user_rounds 低频复核。
    // user_rounds 在 IP-1 时刻是上一轮完成后的累计值（本轮未 tick），0 % N == 0 天然含首轮。
    let topic_id = ctx.topic_id.clone();
    let topic = match topic_id.as_ref() {
        Some(id) => hooks.assistant.topics()?.get(id)?,
        None => None,
    };
    let user_rounds = topic
        .as_ref()
        .map(|t| read_assistant_state(t).user_rounds)
        .unwrap_or(0);
    if !need_user_round_judgement(topic.is_some(), user_rounds) {
        tracing::info!(
            phase = "user_round_judgement_hook",
            topic_id = ?topic_id,
            user_rounds,
            "skip: judgement gated"
        );
        return Ok(());
    }
    // 同源：用本轮主对话同一模型（用户所选），不读配置默认。
    let model = &ctx.model;
    // 打分侧输入：仅已绑定课题且绑定本会话时收集上一介入区间的盖章神经元
    // （用户输入尚未落库，以列表末尾为锚点推导「上次介入（不含）之后」）。
    let neuron_ids = topic
        .as_ref()
        .filter(|t| t.session_id.as_deref() == Some(ctx.session_id.as_str()))
        .and_then(|_| {
            hooks
                .assistant
                .store
                .require_conversation(&ctx.session_id)
                .ok()
                .map(|c| interval_neuron_ids(&c.messages, c.messages.len()))
        })
        .unwrap_or_default();
    let unfinished = hooks.assistant.topics()?.list_unfinished()?;
    tracing::info!(
        phase = "user_round_judgement_hook",
        topic_id = ?topic_id,
        user_rounds,
        neurons = neuron_ids.len(),
        unfinished = unfinished.len(),
        "calling user-round-judgement model"
    );
    let def = &INSTANCE.def;
    // before hook：用户消息尚未落库，锚点 = 当前消息列表末尾（用户消息即将落库的位置）。
    let anchor = JudgementAnchor {
        conversation_id: ctx.session_id.clone(),
        anchor_message_index: Some(ctx.messages.len() as i64),
    };
    // 统一入口接管纠偏（A/B/C）；降级态（score=0 / action=none）由下方按中性语义跳过。
    let outcome = hooks
        .assistant
        .call_judgement(
            def,
            anchor,
            json!({
                "user_input": ctx.model_input,
                "current_session_id": ctx.session_id,
                "topic_id": topic_id,
                "neuron_ids": neuron_ids,
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

    // ① 打分消费：仅 score ∈ -5..=5 且非 0 时应用；0（含降级兜底）/越界/缺失 → warn + skip。
    if let Some(topic) = topic.as_ref() {
        match decision.get("score").and_then(|v| v.as_i64()) {
            Some(score) if score != 0 && (-5..=5).contains(&score) => {
                if neuron_ids.is_empty() {
                    tracing::info!(
                        phase = "user_round_judgement_hook",
                        "skip scoring: last interval has no stamped neuron"
                    );
                } else {
                    tracing::info!(
                        phase = "user_round_judgement_hook",
                        score,
                        "applying weight delta"
                    );
                    hooks
                        .assistant
                        .apply_score_feedback(&topic.id, neuron_ids.clone(), score as f64)
                        .await?;
                }
            }
            Some(score) => tracing::warn!(
                phase = "user_round_judgement_hook",
                score,
                "score not applicable (0 or out of -5..=5); skip scoring"
            ),
            None => tracing::warn!(
                phase = "user_round_judgement_hook",
                "score missing; skip scoring"
            ),
        }
    }

    // ② 路由消费：switch / create / none（分支逻辑自原 match_topic 迁移）。
    let action = decision
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("none");
    tracing::info!(phase = "user_round_judgement_hook", action, "routing decision");
    match action {
        "switch" => {
            let Some(target_id) = decision.get("topic_id").and_then(|v| v.as_str()) else {
                tracing::warn!(
                    phase = "user_round_judgement_hook",
                    "switch missing topic_id; treated as none"
                );
                return Ok(());
            };
            let target = match hooks.assistant.topics()?.get(target_id)? {
                Some(topic) => topic,
                None => {
                    let created = hooks
                        .create_bound_topic_from_decision(ctx, decision, true)
                        .or_else(|error| {
                            tracing::warn!(
                                phase = "user_round_judgement_hook",
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
                        phase = "user_round_judgement_hook",
                        requested_topic_id = target_id,
                        created_topic_id = %created.id,
                        "switch target missing; created topic"
                    );
                    ctx.topic_id = Some(created.id);
                    return Ok(());
                }
            };
            if let Some(bound_session) = target.session_id.clone() {
                if bound_session != ctx.session_id {
                    // 切换到目标课题绑定的会话：runner 检测到 session_id 变化后自动 reload。
                    tracing::info!(
                        phase = "user_round_judgement_hook",
                        from_session = %ctx.session_id,
                        to_session = %bound_session,
                        topic_id = %target.id,
                        "switching session"
                    );
                    ctx.session_id = bound_session;
                    ctx.topic_id = Some(target.id);
                } else {
                    ctx.topic_id = Some(target.id);
                }
            } else {
                let bound = hooks
                    .assistant
                    .topics()?
                    .bind_session(&target.id, &ctx.session_id)?;
                ctx.topic_id = Some(bound.id);
            }
        }
        "none" => {
            // 中性语义（A 降级兜底 action=none）：不创建、不切换，保持当前绑定。
            tracing::info!(
                phase = "user_round_judgement_hook",
                "routing decision: none (no-op)"
            );
        }
        _ => {
            if ctx.topic_id.is_none() {
                match hooks.create_bound_topic_from_decision(ctx, decision, false) {
                    Ok(created) => {
                        tracing::info!(
                            phase = "user_round_judgement_hook",
                            topic_id = %created.id,
                            scope_items = created.scope_in.len(),
                            "created bound topic with scope_in"
                        );
                        ctx.topic_id = Some(created.id);
                    }
                    Err(error) => {
                        tracing::warn!(
                            phase = "user_round_judgement_hook",
                            error = %error,
                            "create topic failed; keep session unbound"
                        );
                    }
                }
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

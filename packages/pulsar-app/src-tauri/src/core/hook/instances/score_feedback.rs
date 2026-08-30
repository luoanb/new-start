//! （休眠）IP-1 · 介入区间打分 hook：对上一介入区间的盖章神经元打分（-5..=5，非 0）。
//!
//! 已被 [`super::user_round_judgement`]（合并裁决）取代，整体保留为休眠单元；
//! 回切 = 本实例移入 `registry::ACTIVE_HOOKS`（inserts 契约与神经元种子仍在原位）。

use std::borrow::Cow;

use serde_json::{json, Value};

use crate::core::assistant_session::{interval_neuron_ids, AssistantHooks};
use crate::core::conversation_runner::RoundContext;
use crate::core::error::{AppError, AppResult};
use crate::core::hook::defs::{BoxFuture, InjectPointId};
use crate::core::hook::judgement::{HookDef, JudgementAnchor, JudgementStatus};
use crate::core::hook::registry::{HookInstance, HookRun};
use crate::core::log_phase::PHASE_HOOK_SCORE_FEEDBACK;
use crate::core::openai_compat::ResponseFormatSpec;

pub const SYSTEM_TYPE_SCORE_FEEDBACK: &str = "assistant_score_feedback";

pub const SCORE_FEEDBACK_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "score": { "type": "integer", "minimum": -5, "maximum": 5 }
  },
  "required": ["score"],
  "additionalProperties": false
}"#;

fn fallback_score_feedback() -> Value {
    // 中性占位（消费处按终态 status 跳过，不应用打分）。
    json!({ "score": 0 })
}

pub(crate) const INSTANCE: HookInstance = HookInstance {
    def: HookDef {
        system_type: SYSTEM_TYPE_SCORE_FEEDBACK,
        label: "hook.scoreFeedback",
        inject_point: InjectPointId::AfterLoadContext.as_str(),
        response_format: Some(ResponseFormatSpec::JsonSchema {
            name: Cow::Borrowed("score_feedback"),
            schema: Cow::Borrowed(SCORE_FEEDBACK_SCHEMA),
        }),
        neutral_fallback: fallback_score_feedback,
    },
    run: HookRun::Before(run_boxed),
};

pub(crate) async fn run(hooks: &AssistantHooks<'_>, ctx: &mut RoundContext) -> AppResult<()> {
    let Some(topic_id) = ctx.topic_id.clone() else {
        tracing::info!(phase = PHASE_HOOK_SCORE_FEEDBACK, "skip: no topic bound yet");
        return Ok(());
    };
    let topic = match hooks.assistant.topics()?.get(&topic_id)? {
        Some(topic) => topic,
        None => {
            tracing::info!(phase = PHASE_HOOK_SCORE_FEEDBACK, topic_id = %topic_id, "skip: topic missing");
            return Ok(());
        }
    };
    let Some(session_id) = topic.session_id.clone() else {
        tracing::info!(phase = PHASE_HOOK_SCORE_FEEDBACK, topic_id = %topic_id, "skip: topic not bound to session");
        return Ok(());
    };
    let conversation = match hooks.assistant.store.require_conversation(&session_id) {
        Ok(conversation) => conversation,
        Err(_) => {
            tracing::info!(phase = PHASE_HOOK_SCORE_FEEDBACK, topic_id = %topic_id, "skip: conversation missing");
            return Ok(());
        }
    };
    // 用户输入在 before hook 之后才落库，本次介入尚未进入消息列表；
    // 以列表末尾为锚点推导「上次介入（不含）之后」的盖章神经元。
    let neuron_ids = interval_neuron_ids(&conversation.messages, conversation.messages.len());
    if neuron_ids.is_empty() {
        tracing::info!(
            phase = PHASE_HOOK_SCORE_FEEDBACK,
            topic_id = %topic_id,
            "skip: last interval has no stamped neuron"
        );
        return Ok(());
    }
    tracing::info!(
        phase = PHASE_HOOK_SCORE_FEEDBACK,
        topic_id = %topic_id,
        neuron_count = neuron_ids.len(),
        "scoring last intervention interval"
    );
    // 同源：用本轮主对话同一模型（用户所选），不读配置默认。
    let model = &ctx.model;
    let def = &INSTANCE.def;
    // before hook：用户消息尚未落库，锚点 = 当前消息列表末尾（用户消息即将落库的位置）。
    let anchor = JudgementAnchor {
        conversation_id: session_id.clone(),
        anchor_message_index: Some(ctx.messages.len() as i64),
    };
    // 统一入口接管纠偏（A/B/C）；降级态（score=0）由下方按终态跳过打分。
    let outcome = hooks
        .assistant
        .call_judgement(
            def,
            anchor,
            json!({
                "user_input": ctx.model_input,
                "topic_id": topic_id,
                "neuron_ids": neuron_ids,
            }),
            &model,
            &ctx.messages,
        )
        .await?;
    if outcome.status == JudgementStatus::Downgraded {
        // A 降级兜底：中性占位（score=0），跳过打分，不让评分副作用阻断主对话。
        tracing::warn!(
            phase = PHASE_HOOK_SCORE_FEEDBACK,
            topic_id = %topic_id,
            error = ?outcome.error,
            "judgement degraded; skip scoring"
        );
        return Ok(());
    }
    let score = outcome
        .decision
        .get("score")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| AppError::InvalidInput("score feedback missing score".into()))?;
    if score == 0 || !(-5..=5).contains(&score) {
        return Err(AppError::InvalidInput(format!(
            "score must be in -5..=5 and non-zero, got {score}"
        )));
    }
    tracing::info!(phase = PHASE_HOOK_SCORE_FEEDBACK, score, "applying weight delta");
    hooks
        .assistant
        .apply_score_feedback(&topic_id, neuron_ids, score as f64)
        .await
}

fn run_boxed<'a>(
    hooks: &'a AssistantHooks<'a>,
    ctx: &'a mut RoundContext,
) -> BoxFuture<'a, AppResult<()>> {
    Box::pin(run(hooks, ctx))
}

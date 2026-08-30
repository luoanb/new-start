//! 压缩 hook（IP-2，AfterPersistInput）：把 `Compactor` 封装为注入点 hook。
//!
//! 位置语义：wire 已在 `persist_input` 落库，此处改写 `ctx.messages` 只影响本次发送、
//! 不动真相源；失败策略为 ignore（注入点吞错，Err 按原 wire 发送）。
//!
//! 实现：以 `ctx.messages` 构造临时会话 → 估算 token 超阈值（模型 context_window ×
//! `threshold_ratio`）→ `Compactor::ensure_fits` 生成摘要并插入消息头 → 替换
//! `ctx.messages`。`project_history` 遇 `Compaction` 会跳过 `summary_of` 覆盖的旧消息
//! （见 `model_call_input`），因此压缩后本轮模型输入显著下降。

use crate::core::{
    compactor::Compactor,
    error::{AppError, AppResult},
    hook::defs::{HookDef, HookHandler, HookRegistry, InjectPointId},
    log_phase::PHASE_HOOK_COMPACTION,
    models::Conversation,
    providers::ProviderRegistry,
};

/// 模型未声明 context_window 时的兜底窗口（token）。
const FALLBACK_CONTEXT_WINDOW: u32 = 128_000;

/// 装配期注册压缩 hook：`Compactor` + `ProviderRegistry` 由上层注入，模型标识取
/// `ctx.model`（与主对话同源，保证同一模型上估算）。
pub fn register(
    registry: &HookRegistry,
    compactor: Compactor,
    providers: ProviderRegistry,
) -> AppResult<()> {
    registry
        .register(HookDef {
            id: "core.compaction",
            label: "自动压缩（IP-2：超阈值生成摘要替换本次 wire）",
            inject_point: InjectPointId::AfterPersistInput,
            handler: HookHandler::AfterPersistInput(Box::new(move |ctx| {
                let compactor = compactor.clone();
                let providers = providers.clone();
                Box::pin(async move {
                    let window = providers
                        .model_context_window(&ctx.model.provider_id, &ctx.model.model_id)
                        .unwrap_or(FALLBACK_CONTEXT_WINDOW);
                    let mut conversation = Conversation {
                        id: ctx.session_id.clone(),
                        mode: ctx.mode.clone(),
                        messages: std::mem::take(&mut ctx.messages),
                        created_at: 0,
                        updated_at: 0,
                        extra: None,
                    };
                    match compactor
                        .ensure_fits(&mut conversation, &providers, &ctx.model, window)
                        .await
                    {
                        Ok(compacted) => {
                            // 无论是否压缩，都放回消息（ensure_fits 不删除原消息，
                            // 压缩仅在头部插入 Compaction 摘要）。
                            ctx.messages = conversation.messages;
                            if compacted {
                                tracing::info!(
                                    phase = PHASE_HOOK_COMPACTION,
                                    session_id = %ctx.session_id,
                                    "wire compacted for this round"
                                );
                            }
                            Ok(())
                        }
                        Err(e) => {
                            // ignore 策略：恢复原 wire，错误由注入点吞掉按原样发送。
                            ctx.messages = conversation.messages;
                            Err(e)
                        }
                    }
                })
            })),
        })
        .map_err(|e| AppError::RuntimeError(format!("register compaction hook failed: {e}")))
}

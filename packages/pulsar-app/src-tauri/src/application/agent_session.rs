use std::sync::Arc;

use crate::core::{
    round_service::ConversationRunner,
    error::AppResult,
    models::{ChatModelSelection, ChatResponse},
    round_contract::{
        ConversationId, InputRecord, RoundDriver, RoundMode, RoundRequest, RoundService, StreamDelta,
    },
};
use super::drivers::AgentDriver;

/// Agent 业务接入：授权由核心按 `RoundMode::Agent` 默认取目录全量，循环推进直到收敛（无工具调用）。
///
/// 阻塞与流式路径的循环策略均在 [`AgentDriver`]（应用驱动层，依赖 `dyn RoundService`）：
/// 首轮落 user 消息，后续轮由 `InputRecord::Continue` 注入继续指令（不重复落库）；
/// 流式多轮共享同一回调，收敛判据与阻塞版一致（`tool_calls_declared`）。
#[derive(Debug, Clone)]
pub struct AgentSession {
    runner: ConversationRunner,
    driver: AgentDriver,
}

impl AgentSession {
    pub fn new(runner: ConversationRunner) -> Self {
        let service: Arc<dyn RoundService> = Arc::new(runner.clone());
        Self {
            runner,
            driver: AgentDriver::new(service),
        }
    }

    pub async fn agent_loop(
        &self,
        session_id: &str,
        input: &str,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        // 模型选型归会话：run 前把调用方传入的模型写回会话态（核心据此决定本轮调用模型）。
        self.runner.set_session_model(session_id, model)?;
        let outcome = self
            .driver
            .run(RoundRequest {
                session_id: ConversationId::new(session_id),
                input: InputRecord::User(input.to_string()),
                mode: RoundMode::Agent,
            })
            .await?;
        Ok(outcome.response)
    }

    /// 流式版 `agent_loop`：经 [`AgentDriver::run_stream`] 发起，多轮循环共享同一 `on_delta`
    /// 回调（每轮占位消息逐块回调，`done: true` 由 Gateway 转发为收敛事件）。
    /// 工具执行仍阻塞，语义与阻塞版一致。
    pub async fn agent_loop_stream(
        &self,
        session_id: &str,
        input: &str,
        model: &ChatModelSelection,
        on_delta: Option<Box<dyn FnMut(StreamDelta) + Send>>,
    ) -> AppResult<ChatResponse> {
        // 模型选型归会话：run 前把调用方传入的模型写回会话态（核心按 Agent 模式取目录全量授权）。
        self.runner.set_session_model(session_id, model)?;
        let on_delta = on_delta.unwrap_or_else(|| Box::new(|_| {}));
        let outcome = self
            .driver
            .run_stream(
                RoundRequest {
                    session_id: ConversationId::new(session_id),
                    input: InputRecord::User(input.to_string()),
                    mode: RoundMode::Agent,
                },
                on_delta,
            )
            .await?;
        Ok(outcome.response)
    }
}

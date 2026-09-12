use std::sync::Arc;

use crate::core::{
    round_service::ConversationRunner,
    error::AppResult,
    models::{ChatModelSelection, ChatResponse},
    round_contract::{
        ConversationId, InputRecord, RoundDriver, RoundMode, RoundRequest, RoundService, StreamDelta,
    },
};
use super::drivers::ChatDriver;

/// Chat 业务接入：无 hooks，单轮直调（退化形态，无选型 / 无工具）。
///
/// 阻塞与流式路径均经 [`ChatDriver`](super::drivers::ChatDriver)（应用驱动层，
/// 依赖 `dyn RoundService`）发起。
#[derive(Debug, Clone)]
pub struct ChatSession {
    runner: ConversationRunner,
    driver: ChatDriver,
}

impl ChatSession {
    pub fn new(runner: ConversationRunner) -> Self {
        let service: Arc<dyn RoundService> = Arc::new(runner.clone());
        Self {
            runner,
            driver: ChatDriver::new(service),
        }
    }

    pub async fn send(
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
                mode: RoundMode::Chat,
            })
            .await?;
        Ok(outcome.response)
    }

    /// 流式版 `send`：经驱动发起，逐块回调 `on_delta`（Gateway 转发为 `MessageDelta`）。
    pub async fn send_stream(
        &self,
        session_id: &str,
        input: &str,
        model: &ChatModelSelection,
        on_delta: Option<Box<dyn FnMut(StreamDelta) + Send>>,
    ) -> AppResult<ChatResponse> {
        self.runner.set_session_model(session_id, model)?;
        let on_delta = on_delta.unwrap_or_else(|| Box::new(|_| {}));
        let outcome = self
            .driver
            .run_stream(
                RoundRequest {
                    session_id: ConversationId::new(session_id),
                    input: InputRecord::User(input.to_string()),
                    mode: RoundMode::Chat,
                },
                on_delta,
            )
            .await?;
        Ok(outcome.response)
    }
}

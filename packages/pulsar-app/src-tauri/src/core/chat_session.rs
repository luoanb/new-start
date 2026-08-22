use super::{
    conversation_runner::{ConversationRunner, InputRecord, StreamDelta},
    error::AppResult,
    models::{ChatModelSelection, ChatResponse},
};

/// Chat 业务接入：无 hooks，单轮直调（退化形态，无选型 / 无工具）。
#[derive(Debug, Clone)]
pub struct ChatSession {
    runner: ConversationRunner,
}

impl ChatSession {
    pub fn new(runner: ConversationRunner) -> Self {
        Self { runner }
    }

    pub async fn send(
        &self,
        session_id: &str,
        input: &str,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        self.runner
            .run_round(
                session_id,
                InputRecord::User(input.to_string()),
                None,
                model,
                None, // 用户聊天窗口发起：保留思考配置（跟随前端勾选）
            )
            .await
    }

    /// 流式版 `send`：逐块回调 `on_delta`（Gateway 转发为 `MessageDelta`）。
    pub async fn send_stream(
        &self,
        session_id: &str,
        input: &str,
        model: &ChatModelSelection,
        on_delta: Option<Box<dyn FnMut(StreamDelta) + Send>>,
    ) -> AppResult<ChatResponse> {
        self.runner
            .run_round_stream(
                session_id,
                InputRecord::User(input.to_string()),
                None,
                model,
                None, // 用户聊天窗口发起：保留思考配置（跟随前端勾选）
                on_delta,
            )
            .await
    }
}

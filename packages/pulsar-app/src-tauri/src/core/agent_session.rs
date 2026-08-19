use std::sync::{Arc, RwLock};

use super::{
    conversation_runner::{ConversationRunner, InputRecord, StreamDelta},
    error::{AppError, AppResult},
    models::{ChatModelSelection, ChatResponse},
    tool_registry::ToolRegistry,
};

/// Agent 工具循环护栏（随 `Engine::agent_mode` 迁入业务独立文件）。
const AGENT_MAX_ITERATIONS: u32 = 20;
const AGENT_CONTINUE_PROMPT: &str = "Continue the agent loop using the latest tool results.";

/// Agent 业务接入：授权注册表全部工具，循环推进直到收敛（无工具调用）；
/// 超 `AGENT_MAX_ITERATIONS` 报错。首轮落 user 消息，后续轮由 `InputRecord::Continue`
/// 注入继续指令（不重复落库）。
#[derive(Debug, Clone)]
pub struct AgentSession {
    runner: ConversationRunner,
    tool_registry: Arc<RwLock<ToolRegistry>>,
}

impl AgentSession {
    pub fn new(runner: ConversationRunner, tool_registry: Arc<RwLock<ToolRegistry>>) -> Self {
        Self {
            runner,
            tool_registry,
        }
    }

    pub async fn agent_loop(
        &self,
        session_id: &str,
        input: &str,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        // 与 Engine.agent_mode 语义一致：注册表全部工具。
        let authorized_tool_ids = self
            .tool_registry
            .read()
            .map(|reg| {
                reg.list_definitions()
                    .into_iter()
                    .map(|d| d.name)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut iterations = 0u32;
        let mut first_round = true;
        loop {
            iterations += 1;
            if iterations > AGENT_MAX_ITERATIONS {
                return Err(AppError::AgentMaxIterations(format!(
                    "Agent exceeded max iterations ({})",
                    AGENT_MAX_ITERATIONS
                )));
            }
            let record = if first_round {
                InputRecord::User(input.to_string())
            } else {
                InputRecord::Continue(AGENT_CONTINUE_PROMPT.to_string())
            };
            let response = self
                .runner
                .run_round(
                    session_id,
                    record,
                    Some(authorized_tool_ids.clone()),
                    model,
                    None,
                    None, // 用户聊天窗口发起的 agent 循环：保留思考配置（跟随前端勾选）
                )
                .await?;
            first_round = false;
            // 本轮执行了工具 → 继续循环（历史已含 tool_call + tool_result）；否则收敛。
            if !self.runner.last_message_is_tool_result(session_id)? {
                return Ok(response);
            }
        }
    }

    /// 流式版 `agent_loop`：多轮循环共享同一 `on_delta` 回调（每轮占位消息逐块回调，
    /// `done: true` 由 Gateway 转发为收敛事件）。工具执行仍阻塞，语义与阻塞版一致。
    pub async fn agent_loop_stream(
        &self,
        session_id: &str,
        input: &str,
        model: &ChatModelSelection,
        on_delta: Option<Box<dyn FnMut(StreamDelta) + Send>>,
    ) -> AppResult<ChatResponse> {
        // 与 Engine.agent_mode 语义一致：注册表全部工具。
        let authorized_tool_ids = self
            .tool_registry
            .read()
            .map(|reg| {
                reg.list_definitions()
                    .into_iter()
                    .map(|d| d.name)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // 多轮共享回调：`run_round_stream` 每次调用独占 `on_delta`，此处用 Arc<Mutex> 共享。
        let shared = on_delta.map(|cb| std::sync::Arc::new(std::sync::Mutex::new(cb)));

        let mut iterations = 0u32;
        let mut first_round = true;
        loop {
            iterations += 1;
            if iterations > AGENT_MAX_ITERATIONS {
                return Err(AppError::AgentMaxIterations(format!(
                    "Agent exceeded max iterations ({})",
                    AGENT_MAX_ITERATIONS
                )));
            }
            let record = if first_round {
                InputRecord::User(input.to_string())
            } else {
                InputRecord::Continue(AGENT_CONTINUE_PROMPT.to_string())
            };
            let round_cb = shared.as_ref().map(|s| {
                let s = std::sync::Arc::clone(s);
                Box::new(move |delta: StreamDelta| {
                    let mut guard = s.lock().unwrap();
                    guard(delta);
                }) as Box<dyn FnMut(StreamDelta) + Send>
            });
            let response = self
                .runner
                .run_round_stream(
                    session_id,
                    record,
                    Some(authorized_tool_ids.clone()),
                    model,
                    None,
                    None, // 用户聊天窗口发起的 agent 循环：保留思考配置（跟随前端勾选）
                    round_cb,
                )
                .await?;
            first_round = false;
            // 本轮执行了工具 → 继续循环（历史已含 tool_call + tool_result）；否则收敛。
            if !self.runner.last_message_is_tool_result(session_id)? {
                return Ok(response);
            }
        }
    }
}

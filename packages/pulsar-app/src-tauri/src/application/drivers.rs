//! 应用驱动层（架构设计 §5.1）：驱动器只拥有循环策略，不拥有轮次实现。
//!
//! 四驱动共享同一 [`RoundService`]（封闭核心唯一入口），不建立继承层次；循环 / 节拍
//! 策略变化不再修改核心。续轮输入只能使用核心承认的类别（用户输入 / Nudge /
//! Continue 指令），不得直接改写已落库消息。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::core::{
    error::{AppError, AppResult},
    round_contract::{
        InputRecord, RoundDriver, RoundOutcome, RoundRequest, RoundService, RoundStatus, StreamDelta,
    },
};

/// Agent 工具循环护栏：单次 agent loop 的最大轮数。
pub(crate) const AGENT_MAX_ITERATIONS: u32 = 20;
/// Agent 续轮固定指令（不落库，仅进入本轮模型输入）。
pub(crate) const AGENT_CONTINUE_PROMPT: &str =
    "Continue the agent loop using the latest tool results.";

/// Chat 驱动：单轮即返回，不续轮。
#[derive(Clone)]
pub struct ChatDriver {
    service: Arc<dyn RoundService>,
}

impl std::fmt::Debug for ChatDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatDriver").finish_non_exhaustive()
    }
}

impl ChatDriver {
    pub fn new(service: Arc<dyn RoundService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl RoundDriver for ChatDriver {
    async fn run(&self, first: RoundRequest) -> AppResult<RoundOutcome> {
        self.service.run(first).await
    }

    async fn run_stream(
        &self,
        first: RoundRequest,
        on_delta: Box<dyn FnMut(StreamDelta) + Send>,
    ) -> AppResult<RoundOutcome> {
        self.service.run_stream(first, on_delta).await
    }
}

/// Assistant 驱动：单轮直通；业务副作用（课题裁决 / 简报 / 复盘 / 计数）全部经由
/// IP-1 / IP-5 Hook 承载，不自带循环。
#[derive(Clone)]
pub struct AssistantDriver {
    service: Arc<dyn RoundService>,
}

impl std::fmt::Debug for AssistantDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssistantDriver").finish_non_exhaustive()
    }
}

impl AssistantDriver {
    pub fn new(service: Arc<dyn RoundService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl RoundDriver for AssistantDriver {
    async fn run(&self, first: RoundRequest) -> AppResult<RoundOutcome> {
        self.service.run(first).await
    }

    async fn run_stream(
        &self,
        first: RoundRequest,
        on_delta: Box<dyn FnMut(StreamDelta) + Send>,
    ) -> AppResult<RoundOutcome> {
        self.service.run_stream(first, on_delta).await
    }
}

/// Poller 驱动：发起 Nudge 轮次（Nudge 落库；同会话忙时核心跳过）。节拍调度由外侧
/// poller 基础设施（poller / poller_step）驱动，本驱动只负责单次 Nudge 轮。
#[derive(Clone)]
pub struct PollerDriver {
    service: Arc<dyn RoundService>,
}

impl std::fmt::Debug for PollerDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PollerDriver").finish_non_exhaustive()
    }
}

impl PollerDriver {
    pub fn new(service: Arc<dyn RoundService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl RoundDriver for PollerDriver {
    async fn run(&self, first: RoundRequest) -> AppResult<RoundOutcome> {
        self.service.run(first).await
    }

    async fn run_stream(
        &self,
        first: RoundRequest,
        on_delta: Box<dyn FnMut(StreamDelta) + Send>,
    ) -> AppResult<RoundOutcome> {
        self.service.run_stream(first, on_delta).await
    }
}

/// Agent 驱动：`tool_calls_declared` 为真时以 `InputRecord::Continue` 固定指令续轮
/// （不落库，仅进入本轮模型输入），直到本轮不再声明能力调用、轮次被取消 / 跳过，
/// 或达到 [`AGENT_MAX_ITERATIONS`]。授权由核心按 `RoundMode::Agent` 默认取目录全量。
#[derive(Clone)]
pub struct AgentDriver {
    service: Arc<dyn RoundService>,
}

impl std::fmt::Debug for AgentDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentDriver").finish_non_exhaustive()
    }
}

impl AgentDriver {
    pub fn new(service: Arc<dyn RoundService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl RoundDriver for AgentDriver {
    async fn run(&self, first: RoundRequest) -> AppResult<RoundOutcome> {
        let mut request = first;
        let mut iterations = 0u32;
        loop {
            iterations += 1;
            if iterations > AGENT_MAX_ITERATIONS {
                return Err(AppError::AgentMaxIterations(format!(
                    "Agent exceeded max iterations ({})",
                    AGENT_MAX_ITERATIONS
                )));
            }
            let outcome = self.service.run(request.clone()).await?;
            // 被取消 / 跳过 → 不再续轮（取消语义：不启动新的外部调用）。
            if outcome.status != RoundStatus::Completed {
                return Ok(outcome);
            }
            // 收敛：本轮未声明能力调用。与原「末条消息非工具结果」判据等价——
            // 非空声明必然产生成对结果（见执行面）。
            if !outcome.tool_calls_declared {
                return Ok(outcome);
            }
            request = RoundRequest {
                input: InputRecord::Continue(AGENT_CONTINUE_PROMPT.to_string()),
                ..request
            };
        }
    }

    /// 流式版：循环策略、收敛判据（`tool_calls_declared`）、20 轮上限、取消 / 跳过不续轮
    /// 与阻塞版 `run` 完全一致；回调在轮间共享（每轮传入一个转发回调），逐轮占位消息的
    /// 增量都经过同一 `on_delta`。
    async fn run_stream(
        &self,
        first: RoundRequest,
        on_delta: Box<dyn FnMut(StreamDelta) + Send>,
    ) -> AppResult<RoundOutcome> {
        let shared = Arc::new(Mutex::new(on_delta));
        let mut request = first;
        let mut iterations = 0u32;
        loop {
            iterations += 1;
            if iterations > AGENT_MAX_ITERATIONS {
                return Err(AppError::AgentMaxIterations(format!(
                    "Agent exceeded max iterations ({})",
                    AGENT_MAX_ITERATIONS
                )));
            }
            let round_cb: Box<dyn FnMut(StreamDelta) + Send> = {
                let shared = Arc::clone(&shared);
                Box::new(move |delta: StreamDelta| {
                    let mut guard = shared.lock().unwrap();
                    guard(delta);
                })
            };
            let outcome = self.service.run_stream(request.clone(), round_cb).await?;
            // 被取消 / 跳过 → 不再续轮（取消语义：不启动新的外部调用）。
            if outcome.status != RoundStatus::Completed {
                return Ok(outcome);
            }
            // 收敛：本轮未声明能力调用（与阻塞版同一判据）。
            if !outcome.tool_calls_declared {
                return Ok(outcome);
            }
            request = RoundRequest {
                input: InputRecord::Continue(AGENT_CONTINUE_PROMPT.to_string()),
                ..request
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;
    use crate::core::models::ChatResponse;
    use crate::core::round_contract::{ConversationId, RoundMode};

    /// 脚本化服务替身：按序回放预置结果并记录全部请求（驱动策略测试不触真实核心）。
    struct ScriptedService {
        outcomes: Mutex<VecDeque<RoundOutcome>>,
        requests: Mutex<Vec<RoundRequest>>,
    }

    impl ScriptedService {
        fn new(outcomes: impl IntoIterator<Item = RoundOutcome>) -> Self {
            Self {
                outcomes: Mutex::new(outcomes.into_iter().collect()),
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl RoundService for ScriptedService {
        async fn run(&self, request: RoundRequest) -> AppResult<RoundOutcome> {
            self.requests.lock().unwrap().push(request);
            self.outcomes
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| AppError::RuntimeError("script exhausted".into()))
        }

        async fn run_stream(
            &self,
            request: RoundRequest,
            _on_delta: Box<dyn FnMut(StreamDelta) + Send>,
        ) -> AppResult<RoundOutcome> {
            self.requests.lock().unwrap().push(request);
            self.outcomes
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| AppError::RuntimeError("script exhausted".into()))
        }

        async fn cancel(&self, _session_id: &ConversationId) -> AppResult<()> {
            Ok(())
        }
    }

    fn agent_request(input: InputRecord) -> RoundRequest {
        RoundRequest {
            session_id: ConversationId::new("s1"),
            input,
            mode: RoundMode::Agent,
        }
    }

    fn outcome(tool_calls_declared: bool, status: RoundStatus) -> RoundOutcome {
        RoundOutcome {
            session_id: ConversationId::new("s1"),
            response: ChatResponse {
                conversation_id: "s1".into(),
                response: String::new(),
            },
            tool_calls_declared,
            status,
        }
    }

    fn agent_driver(service: &Arc<ScriptedService>) -> AgentDriver {
        AgentDriver::new(Arc::clone(service) as Arc<dyn RoundService>)
    }

    #[tokio::test]
    async fn agent_driver_loops_on_declared_tools_and_builds_continue_input() {
        let service = Arc::new(ScriptedService::new([
            outcome(true, RoundStatus::Completed),
            outcome(true, RoundStatus::Completed),
            outcome(false, RoundStatus::Completed),
        ]));
        let driver = agent_driver(&service);
        let result = driver
            .run(agent_request(InputRecord::User("go".into())))
            .await
            .unwrap();
        assert!(!result.tool_calls_declared, "收敛轮返回");
        let requests = service.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(matches!(requests[0].input, InputRecord::User(_)));
        // 续轮 = Continue 固定指令（不落库）；会话 / 模式随请求保持（模型与授权由核心按模式/会话态决定）。
        assert!(matches!(&requests[1].input, InputRecord::Continue(_)));
        assert!(matches!(&requests[2].input, InputRecord::Continue(_)));
        assert_eq!(requests[1].mode, RoundMode::Agent);
        assert_eq!(requests[1].session_id.as_str(), "s1");
    }

    #[tokio::test]
    async fn agent_driver_stops_on_cancelled_round() {
        let service = Arc::new(ScriptedService::new([outcome(
            true,
            RoundStatus::Cancelled,
        )]));
        let driver = agent_driver(&service);
        let result = driver
            .run(agent_request(InputRecord::User("go".into())))
            .await
            .unwrap();
        assert_eq!(result.status, RoundStatus::Cancelled);
        assert_eq!(service.requests.lock().unwrap().len(), 1, "取消后不再续轮");
    }

    #[tokio::test]
    async fn agent_driver_errors_after_max_iterations() {
        let scripted = std::iter::repeat_with(|| outcome(true, RoundStatus::Completed)).take(25);
        let service = Arc::new(ScriptedService::new(scripted));
        let driver = agent_driver(&service);
        let error = driver
            .run(agent_request(InputRecord::User("go".into())))
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::AgentMaxIterations(_)));
        assert_eq!(
            service.requests.lock().unwrap().len(),
            AGENT_MAX_ITERATIONS as usize
        );
    }

    #[tokio::test]
    async fn single_round_drivers_issue_exactly_one_call() {
        for build in [
            |s: Arc<dyn RoundService>| Box::new(ChatDriver::new(s)) as Box<dyn RoundDriver>,
            |s: Arc<dyn RoundService>| Box::new(AssistantDriver::new(s)) as Box<dyn RoundDriver>,
            |s: Arc<dyn RoundService>| Box::new(PollerDriver::new(s)) as Box<dyn RoundDriver>,
        ] {
            let service = Arc::new(ScriptedService::new([outcome(false, RoundStatus::Completed)]));
            build(Arc::clone(&service) as Arc<dyn RoundService>)
                .run(agent_request(InputRecord::User("hi".into())))
                .await
                .unwrap();
            assert_eq!(
                service.requests.lock().unwrap().len(),
                1,
                "单轮驱动不得续轮"
            );
        }
    }
}

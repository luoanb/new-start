//! 轮询步进请求与 Poller 处理器（Assistant 轮询调度壳）。
//!
//! 仅承载「Poller tick → 发送 PollAll 请求」的桥接；实际推进逻辑在
//! `assistant_session.rs` 的 `AssistantSession::step_poller` / `process_step_request`。

use tokio::sync::mpsc::UnboundedSender;

use super::poller::PollHandler;

/// Poller 注册任务名：`Poller` 定时触发后经 channel 转成 `AssistantStepRequest::PollAll`。
pub const ASSISTANT_POLL_TASK: &str = "assistant_advance";

/// 轮询步进请求：当前仅支持全量推进（跨课题受限并发）。
#[derive(Debug, Clone)]
pub enum AssistantStepRequest {
    PollAll,
}

/// Poller 处理器：tick 到期时发送 `PollAll`（不阻塞 tick 循环）。
pub struct AssistantPollHandler {
    pub tx: UnboundedSender<AssistantStepRequest>,
}

impl PollHandler for AssistantPollHandler {
    fn on_tick(&mut self) {
        tracing::info!(
            phase = "assistant_poll_handler",
            "on_tick fired, sending PollAll"
        );
        let _ = self.tx.send(AssistantStepRequest::PollAll);
    }
}

//! 终端会话事件广播器：所有会话输出/退出事件的唯一发布点。
//!
//! 会话事件泵（读线程 → mpsc）统一汇入本 hub：一方面经 `AppHandle.emit`
//! 走桌面 IPC 事件（`app://terminal-output` / `app://terminal-exit`，桌面前端监听），
//! 另一方面经 `broadcast` 供 WebSocket 网关（浏览器前端）订阅，双路转发，
//! 避免同一输出字节流被重复消费。

use std::sync::Arc;

use tauri::{AppHandle, Emitter};
use tokio::sync::{broadcast, mpsc};

use super::commands::{TerminalExitPayload, TerminalOutputPayload};
use super::session::{TERMINAL_EXIT_EVENT, TERMINAL_OUTPUT_EVENT};

/// 会话事件广播器（Clone 即共享同一内部 Sender / AppHandle）。
#[derive(Clone)]
pub struct TerminalEventHub {
    inner: Arc<HubInner>,
}

struct HubInner {
    output: broadcast::Sender<TerminalOutputPayload>,
    exit: broadcast::Sender<TerminalExitPayload>,
    /// None = 无 AppHandle（单元测试场景）：仅广播 WS 订阅者，跳过桌面 IPC 事件。
    app: Option<AppHandle>,
}

impl TerminalEventHub {
    pub fn new(app: AppHandle) -> Self {
        // output 为高频通道（256 足够，Lagged 时前端会跳过缺块，流式语义可接受）；
        // exit 低频（16）。
        let (output, _) = broadcast::channel(256);
        let (exit, _) = broadcast::channel(16);
        Self {
            inner: Arc::new(HubInner {
                output,
                exit,
                app: Some(app),
            }),
        }
    }

    /// 无 AppHandle 的测试构造：仅保留 WS 广播通道，跳过桌面 IPC 事件发射。
    #[cfg(test)]
    pub fn new_for_test() -> Self {
        let (output, _) = broadcast::channel(256);
        let (exit, _) = broadcast::channel(16);
        Self {
            inner: Arc::new(HubInner {
                output,
                exit,
                app: None,
            }),
        }
    }

    /// 发布输出块：桌面 IPC 事件 + WS 订阅者双路广播。
    pub fn publish_output(&self, session_id: &str, data: Vec<u8>) {
        let payload = TerminalOutputPayload {
            session_id: session_id.to_string(),
            data,
        };
        if let Some(app) = &self.inner.app {
            let _ = app.emit(TERMINAL_OUTPUT_EVENT, payload.clone());
        }
        let _ = self.inner.output.send(payload);
    }

    /// 发布退出事件：桌面 IPC 事件 + WS 订阅者双路广播。
    pub fn publish_exit(&self, session_id: &str, exit_code: i32) {
        let payload = TerminalExitPayload {
            session_id: session_id.to_string(),
            exit_code,
        };
        if let Some(app) = &self.inner.app {
            let _ = app.emit(TERMINAL_EXIT_EVENT, payload.clone());
        }
        let _ = self.inner.exit.send(payload);
    }

    pub fn subscribe_output(&self) -> broadcast::Receiver<TerminalOutputPayload> {
        self.inner.output.subscribe()
    }

    pub fn subscribe_exit(&self) -> broadcast::Receiver<TerminalExitPayload> {
        self.inner.exit.subscribe()
    }
}

/// 会话事件泵：消费读线程的 output/exit 通道并发布到 hub。
///
/// 所有产生会话的入口（IPC `terminal_spawn`、WS 网关 spawn、Agent 可见执行）
/// 统一调用本函数，保证桌面 IPC 与浏览器 WS 双端都能收到同一会话的输出。
pub fn pump_session_events(
    hub: TerminalEventHub,
    session_id: String,
    mut output_rx: mpsc::Receiver<Vec<u8>>,
    mut exit_rx: mpsc::Receiver<i32>,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(chunk) = output_rx.recv() => {
                    hub.publish_output(&session_id, chunk);
                }
                code = exit_rx.recv() => {
                    if let Some(code) = code {
                        hub.publish_exit(&session_id, code);
                    }
                    break;
                }
            }
        }
    });
}

//! 通用 WebSocket 公共服务（axum `/ws`）。
//!
//! - 鉴权：复用 `auth_middleware`（与 `/rpc`、`/events` 一致；浏览器 WebSocket 无法自定义
//!   请求头，token 走 `?token=` query，与 SSE 一致）。
//! - 帧信封：`{ topic, ... }`，按 `topic` 分发到业务 handler；v1 仅 `terminal`
//!   （见 `crate::terminal::ws`）。未来新增业务只需注册分发分支。
//! - 业务事件（如终端 output/exit）由本层订阅后带 topic 信封推送，与请求响应共用连接。

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;

use crate::terminal::ws as terminal_ws;

use super::NetState;

/// 帧信封：`{ topic, ... }`；`rest` 为业务载荷（如 `{ type: "spawn", ... }`）。
#[derive(Debug, Deserialize)]
struct Envelope {
    topic: String,
    #[serde(flatten)]
    rest: Value,
}

/// `/ws` 端点：握手（鉴权已在 auth_middleware 完成）后升级为连接任务。
pub async fn handle_ws(State(state): State<NetState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// 单连接处理：请求帧 dispatch + 终端会话事件（output/exit）转发。
async fn handle_socket(socket: WebSocket, state: NetState) {
    let (mut sender, mut receiver) = socket.split();

    // 订阅会话事件：本连接收到的输出与桌面 IPC 一致（同一 hub 广播）。
    let mut output_rx = state.terminal_hub.subscribe_output();
    let mut exit_rx = state.terminal_hub.subscribe_exit();

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let response = dispatch(&text, &state).await;
                        if sender.send(Message::Text(response.into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    // Ping/Pong 由 axum 自动处理；二进制帧 v1 不使用。
                    _ => {}
                }
            }
            result = output_rx.recv() => {
                if let Ok(payload) = result {
                    let frame = terminal_ws::output_frame(&payload.session_id, &payload.data);
                    if sender.send(Message::Text(frame.into())).await.is_err() {
                        break;
                    }
                }
            }
            result = exit_rx.recv() => {
                if let Ok(payload) = result {
                    let frame = terminal_ws::exit_frame(&payload.session_id, payload.exit_code);
                    if sender.send(Message::Text(frame.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
}

/// 信封解析 + topic 分发；解析失败 / 未知 topic 返回 error 帧（不中断连接）。
async fn dispatch(text: &str, state: &NetState) -> String {
    let envelope: Envelope = match serde_json::from_str(text) {
        Ok(e) => e,
        Err(e) => return error_frame(format!("invalid envelope: {e}")),
    };
    match envelope.topic.as_str() {
        terminal_ws::TOPIC => {
            let payload = envelope.rest.to_string();
            terminal_ws::handle_frame(
                &payload,
                &state.terminal,
                &state.terminal_hub,
                &state.gateway.workspace_store(),
            )
            .await
        }
        other => error_frame(format!("unknown topic: {other}")),
    }
}

fn error_frame(message: String) -> String {
    serde_json::json!({ "topic": "_error", "type": "error", "message": message }).to_string()
}

//! WebSocket PTY 网关：让浏览器（非 Tauri）前端也能操作终端会话。
//!
//! - 绑定 `127.0.0.1:<port>`（回环；端口 `PULSAR_TERMINAL_WS_PORT`，默认 43110）。
//! - 与桌面 IPC 共用同一 `TerminalManager` 会话集与 `TerminalEventHub` 输出流。
//! - 帧协议见 spec `docs/specs/2026-08-20_11-30_terminal-browser-ws.md`：
//!   client→server `spawn / write / resize / kill / list`；
//!   server→client `spawned / output / exit / list / error`；
//!   二进制输出以 base64 编码于 JSON 文本帧。
//! - v1 无鉴权（同机回环等价本机终端风险），协议预留 `token` 字段供 v2。

use std::sync::Arc;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

use super::events::{pump_session_events, TerminalEventHub};
use super::manager::TerminalManager;
use super::session::{SessionInfo, TerminalSession};
use crate::core::{AppError, AppResult};

/// 默认 WS 网关端口（`PULSAR_TERMINAL_WS_PORT` 可覆盖）。
pub const DEFAULT_WS_PORT: u16 = 43110;

/// client→server 请求帧。
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum WsRequest {
    Spawn {
        cwd: Option<String>,
        shell: Option<String>,
        cols: Option<u16>,
        rows: Option<u16>,
    },
    Write {
        #[serde(rename = "sessionId")]
        session_id: String,
        /// base64 编码的输入字节（与 output 帧一致，兼容任意二进制输入）。
        data: String,
    },
    Resize {
        #[serde(rename = "sessionId")]
        session_id: String,
        cols: u16,
        rows: u16,
    },
    Kill {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    List,
}

/// 启动 WS 网关（阻塞运行，直到监听失败返回）。
pub async fn run(manager: Arc<TerminalManager>, hub: TerminalEventHub) -> AppResult<()> {
    let port: u16 = std::env::var("PULSAR_TERMINAL_WS_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_WS_PORT);
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| AppError::RuntimeError(format!("terminal ws: bind {addr} failed: {e}")))?;
    tracing::info!(addr = %addr, "terminal ws gateway listening");

    loop {
        let (stream, _peer) = match listener.accept().await {
            Ok(ok) => ok,
            Err(e) => {
                tracing::warn!(error = %e, "terminal ws: accept failed");
                continue;
            }
        };
        let conn_manager = Arc::clone(&manager);
        let conn_hub = hub.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, conn_manager, conn_hub).await {
                tracing::debug!(error = %e, "terminal ws connection closed");
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    manager: Arc<TerminalManager>,
    hub: TerminalEventHub,
) -> Result<(), String> {
    let ws = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(|e| format!("terminal ws: handshake failed: {e}"))?;
    let (mut sink, mut source) = ws.split();

    // 订阅会话事件：本连接收到的输出与桌面 IPC 一致（同一 hub 广播）。
    let mut output_rx = hub.subscribe_output();
    let mut exit_rx = hub.subscribe_exit();

    loop {
        tokio::select! {
            msg = source.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let response = handle_request(&text, &manager, &hub).await;
                        send_text(&mut sink, &response).await?;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    // Ping/Pong 由 tungstenite 自动处理；二进制帧 v1 不使用。
                    _ => {}
                }
            }
            result = output_rx.recv() => {
                if let Ok(payload) = result {
                    let frame = json!({
                        "type": "output",
                        "sessionId": payload.session_id,
                        "data": base64::engine::general_purpose::STANDARD.encode(&payload.data),
                    })
                    .to_string();
                    send_text(&mut sink, &frame).await?;
                }
            }
            result = exit_rx.recv() => {
                if let Ok(payload) = result {
                    let frame = json!({
                        "type": "exit",
                        "sessionId": payload.session_id,
                        "exitCode": payload.exit_code,
                    })
                    .to_string();
                    send_text(&mut sink, &frame).await?;
                }
            }
        }
    }
    Ok(())
}

async fn send_text<S>(sink: &mut S, text: &str) -> Result<(), String>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    sink.send(Message::Text(text.to_string()))
        .await
        .map_err(|e| format!("terminal ws: send failed: {e}"))
}

/// 处理单个请求帧并返回响应帧（含 error 帧，不中断连接）。
async fn handle_request(
    text: &str,
    manager: &Arc<TerminalManager>,
    hub: &TerminalEventHub,
) -> String {
    let request: WsRequest = match serde_json::from_str(text) {
        Ok(req) => req,
        Err(e) => return error_frame(format!("invalid request frame: {e}")),
    };
    match request {
        WsRequest::Spawn { cwd, shell, cols, rows } => {
            match TerminalSession::spawn(cwd, shell, cols, rows) {
                Ok((session, output_rx, exit_rx)) => {
                    let session_id = session.session_id().to_string();
                    manager.insert(Arc::clone(&session));
                    pump_session_events(hub.clone(), session_id.clone(), output_rx, exit_rx);
                    json!({ "type": "spawned", "sessionId": session_id }).to_string()
                }
                Err(e) => error_frame(format!("spawn failed: {e}")),
            }
        }
        WsRequest::Write { session_id, data } => {
            let bytes = match base64::engine::general_purpose::STANDARD.decode(&data) {
                Ok(bytes) => bytes,
                Err(e) => return error_frame(format!("write: invalid base64: {e}")),
            };
            match manager.get(&session_id) {
                Some(session) => match session.write(&bytes) {
                    Ok(()) => json!({ "type": "ok" }).to_string(),
                    Err(e) => error_frame(format!("write failed: {e}")),
                },
                None => error_frame(format!("session not found: {session_id}")),
            }
        }
        WsRequest::Resize { session_id, cols, rows } => {
            match manager.get(&session_id) {
                Some(session) => match session.resize(cols, rows) {
                    Ok(()) => json!({ "type": "ok" }).to_string(),
                    Err(e) => error_frame(format!("resize failed: {e}")),
                },
                None => error_frame(format!("session not found: {session_id}")),
            }
        }
        WsRequest::Kill { session_id } => match manager.get(&session_id) {
            Some(session) => match session.kill() {
                Ok(()) => json!({ "type": "ok" }).to_string(),
                Err(e) => error_frame(format!("kill failed: {e}")),
            },
            None => error_frame(format!("session not found: {session_id}")),
        },
        WsRequest::List => {
            let sessions: Vec<SessionInfo> = manager.list();
            json!({ "type": "list", "sessions": sessions }).to_string()
        }
    }
}

fn error_frame(message: String) -> String {
    json!({ "type": "error", "message": message }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn ws_request_parses_spawn() {
        let req: WsRequest = serde_json::from_str(
            r#"{"type":"spawn","cwd":"/tmp","shell":"sh","cols":100,"rows":30}"#,
        )
        .unwrap();
        match req {
            WsRequest::Spawn { cwd, shell, cols, rows } => {
                assert_eq!(cwd.as_deref(), Some("/tmp"));
                assert_eq!(shell.as_deref(), Some("sh"));
                assert_eq!(cols, Some(100));
                assert_eq!(rows, Some(30));
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn ws_request_parses_write_with_camel_case_fields() {
        let req: WsRequest = serde_json::from_str(
            r#"{"type":"write","sessionId":"term-0001","data":"aGk="}"#,
        )
        .unwrap();
        match req {
            WsRequest::Write { session_id, data } => {
                assert_eq!(session_id, "term-0001");
                assert_eq!(data, "aGk=");
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn error_frame_is_valid_json() {
        let frame = error_frame("boom".into());
        let value: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(value["type"], "error");
        assert_eq!(value["message"], "boom");
    }

    /// 端到端网关测试：真实 TCP + WS 客户端，验证
    /// spawn → spawned → write(echo) → output → kill → exit 全链路帧协议。
    #[tokio::test]
    async fn ws_gateway_roundtrip_spawn_write_output_kill_exit() {
        use std::time::Duration;

        use tokio_tungstenite::tungstenite::Message;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let manager = Arc::new(TerminalManager::default());
        let hub = TerminalEventHub::new_for_test();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, manager, hub).await.unwrap();
        });

        let (ws, _resp) = tokio_tungstenite::connect_async(format!("ws://{addr}"))
            .await
            .expect("ws connect should succeed");
        let (mut sink, mut source) = ws.split();

        // 1) spawn
        sink.send(Message::Text(r#"{"type":"spawn","shell":"sh"}"#.into()))
            .await
            .unwrap();
        let spawned = tokio::time::timeout(Duration::from_secs(3), source.next())
            .await
            .expect("spawned frame within 3s")
            .expect("stream alive")
            .expect("text frame");
        let spawned: Value = serde_json::from_str(spawned.to_text().unwrap()).unwrap();
        assert_eq!(spawned["type"], "spawned");
        let session_id = spawned["sessionId"].as_str().unwrap().to_string();

        // 2) write：echo 一行文本
        let data = base64::engine::general_purpose::STANDARD.encode(b"echo ws-roundtrip\n");
        sink.send(
            Message::Text(
                json!({"type":"write","sessionId":session_id,"data":data})
                    .to_string()
                    .into(),
            ),
        )
        .await
        .unwrap();

        // 3) 收输出帧，直到看到 echo 内容
        let mut saw_output = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !saw_output {
            let remaining = deadline - tokio::time::Instant::now();
            let frame = tokio::time::timeout(remaining, source.next())
                .await
                .expect("output frame within deadline")
                .expect("stream alive")
                .expect("text frame");
            let value: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
            match value["type"].as_str().unwrap() {
                "output" => {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(value["data"].as_str().unwrap())
                        .unwrap();
                    if String::from_utf8_lossy(&bytes).contains("ws-roundtrip") {
                        saw_output = true;
                    }
                }
                // write 的响应帧（ok）先于输出帧到达，属预期
                "ok" => {}
                other => panic!("unexpected frame before output: {other}"),
            }
        }

        // 4) kill：交互 shell 不会自然退出，kill 后应收到 exit 帧
        sink.send(
            Message::Text(
                json!({"type":"kill","sessionId":session_id}).to_string().into(),
            ),
        )
        .await
        .unwrap();
        let mut saw_exit = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !saw_exit {
            let remaining = deadline - tokio::time::Instant::now();
            let frame = tokio::time::timeout(remaining, source.next())
                .await
                .expect("exit frame within deadline")
                .expect("stream alive")
                .expect("text frame");
            let value: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
            match value["type"].as_str().unwrap() {
                "exit" => {
                    assert_eq!(value["sessionId"], session_id, "exit frame session id");
                    saw_exit = true;
                }
                // kill 后可能还有残余输出帧，忽略
                other => assert!(
                    other == "output" || other == "ok",
                    "unexpected frame before exit: {other}"
                ),
            }
        }

        server.await.unwrap();
    }
}

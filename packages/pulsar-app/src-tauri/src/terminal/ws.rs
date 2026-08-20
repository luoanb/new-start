//! 终端 WS 业务 handler：帧协议解析与命令执行（axum `/ws` 公共服务的一个 topic）。
//!
//! - 不持有 TcpListener / tungstenite：连接接入、topic 信封分发、事件转发由
//!   `crate::net::ws` 统一负责；本模块只处理 `topic: "terminal"` 的载荷帧。
//! - 帧协议见 spec `docs/specs/2026-08-20_11-30_terminal-browser-ws.md`：
//!   client→server `spawn / write / resize / kill / list`；
//!   server→client `spawned / output / exit / list / error`；
//!   二进制输出以 base64 编码于 JSON 文本帧；所有帧带 `topic: "terminal"` 信封。
//! - 与桌面 IPC 共用同一 `TerminalManager` 会话集与 `TerminalEventHub` 输出流。

use std::sync::Arc;

use base64::Engine;
use serde::Deserialize;
use serde_json::json;

use super::events::{pump_session_events, TerminalEventHub};
use super::manager::TerminalManager;
use super::session::{SessionInfo, TerminalSession};

/// 本业务的 topic 标识（net::ws 按此分发）。
pub const TOPIC: &str = "terminal";

/// client→server 请求帧（载荷层，不含 topic 信封）。
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

/// 处理单个终端请求载荷，返回带 topic 信封的响应帧（含 error 帧，不中断连接）。
pub async fn handle_frame(
    payload: &str,
    manager: &Arc<TerminalManager>,
    hub: &TerminalEventHub,
) -> String {
    let request: WsRequest = match serde_json::from_str(payload) {
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
                    json!({ "topic": TOPIC, "type": "spawned", "sessionId": session_id }).to_string()
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
                    Ok(()) => json!({ "topic": TOPIC, "type": "ok" }).to_string(),
                    Err(e) => error_frame(format!("write failed: {e}")),
                },
                None => error_frame(format!("session not found: {session_id}")),
            }
        }
        WsRequest::Resize { session_id, cols, rows } => {
            match manager.get(&session_id) {
                Some(session) => match session.resize(cols, rows) {
                    Ok(()) => json!({ "topic": TOPIC, "type": "ok" }).to_string(),
                    Err(e) => error_frame(format!("resize failed: {e}")),
                },
                None => error_frame(format!("session not found: {session_id}")),
            }
        }
        WsRequest::Kill { session_id } => match manager.get(&session_id) {
            Some(session) => match session.kill() {
                Ok(()) => json!({ "topic": TOPIC, "type": "ok" }).to_string(),
                Err(e) => error_frame(format!("kill failed: {e}")),
            },
            None => error_frame(format!("session not found: {session_id}")),
        },
        WsRequest::List => {
            let sessions: Vec<SessionInfo> = manager.list();
            json!({ "topic": TOPIC, "type": "list", "sessions": sessions }).to_string()
        }
    }
}

/// 错误帧（带 topic 信封）。
fn error_frame(message: String) -> String {
    json!({ "topic": TOPIC, "type": "error", "message": message }).to_string()
}

/// 会话输出事件帧（带 topic 信封；由 net::ws 推送给订阅连接）。
pub fn output_frame(session_id: &str, data: &[u8]) -> String {
    json!({
        "topic": TOPIC,
        "type": "output",
        "sessionId": session_id,
        "data": base64::engine::general_purpose::STANDARD.encode(data),
    })
    .to_string()
}

/// 会话退出事件帧（带 topic 信封；由 net::ws 推送给订阅连接）。
pub fn exit_frame(session_id: &str, exit_code: i32) -> String {
    json!({
        "topic": TOPIC,
        "type": "exit",
        "sessionId": session_id,
        "exitCode": exit_code,
    })
    .to_string()
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
    fn error_frame_carries_topic_envelope() {
        let frame = error_frame("boom".into());
        let value: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(value["topic"], TOPIC);
        assert_eq!(value["type"], "error");
        assert_eq!(value["message"], "boom");
    }

    #[test]
    fn output_frame_carries_topic_envelope_and_base64() {
        let frame = output_frame("term-0001", b"hi");
        let value: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(value["topic"], TOPIC);
        assert_eq!(value["type"], "output");
        assert_eq!(value["sessionId"], "term-0001");
        assert_eq!(value["data"], "aGk=");
    }

    #[test]
    fn exit_frame_carries_topic_envelope() {
        let frame = exit_frame("term-0001", 3);
        let value: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(value["topic"], TOPIC);
        assert_eq!(value["type"], "exit");
        assert_eq!(value["sessionId"], "term-0001");
        assert_eq!(value["exitCode"], 3);
    }
}

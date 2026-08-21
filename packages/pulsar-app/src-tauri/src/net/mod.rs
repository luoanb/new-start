//! 内嵌网络服务（远程模式）。
//!
//! 在 Tauri 进程内按 `config.json` 顶层 `server` 节条件启动 axum HTTP server，
//! 复用 `Gateway` 与分域 State，把 54 个 Tauri command 以统一 RPC 端点暴露，
//! 并把 `StateChange` 通过 SSE 推送给远程前端。本机 Tauri IPC 路径不受影响。

pub mod auth;
pub mod rpc;
pub mod sse;
#[cfg(feature = "embed-static")]
pub mod static_assets;
pub mod ws;

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{header, Response},
    middleware,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use tokio::sync::broadcast;

use crate::core::{events::STATE_CHANGED_EVENT, Gateway, StateChange, StateEmitter};
use crate::terminal::{events::TerminalEventHub, manager::TerminalManager};

/// 内嵌 server 运行配置（源自 `config.json` `server` 节，缺省不启动）。
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub tokens: Vec<String>,
}

/// 服务器运行信息（`GET /api/config` 公开端点 + 桌面 IPC `server_info` 共用同一结构）。
/// 只含非敏感运行信息，不含 token 本身。
#[derive(Debug, Clone, Serialize)]
pub struct ServerInfo {
    pub version: &'static str,
    /// server 是否启用（桌面场景反映 config.json `server.enabled`）。
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    /// 是否托管前端静态资源（feature `embed-static`）。
    pub static_enabled: bool,
    /// 是否已配置 token（true 时远程访问需要认证）。
    pub auth_required: bool,
}

/// axum managed state：可 Clone 的 `Gateway` + 状态发射器 + SSE 广播通道 + token 白名单 +
/// 终端会话（WS `/ws` 终端业务复用）+ 实际监听 host/port（供 `/config` 暴露）。
#[derive(Clone)]
pub struct NetState {
    pub gateway: Gateway,
    pub state_emit: StateEmitter,
    pub events_tx: broadcast::Sender<StateChange>,
    pub tokens: Vec<String>,
    pub terminal: Arc<TerminalManager>,
    pub terminal_hub: TerminalEventHub,
    pub host: String,
    pub port: u16,
}

/// `/api/config` 公开端点（免鉴权）：供远程前端同源自动发现与能力探测。
async fn handle_config(State(state): State<NetState>) -> Json<ServerInfo> {
    Json(ServerInfo {
        version: env!("CARGO_PKG_VERSION"),
        enabled: true, // 能访问到本端点即 server 在运行
        host: state.host.clone(),
        port: state.port,
        static_enabled: cfg!(feature = "embed-static"),
        auth_required: !state.tokens.is_empty(),
    })
}

/// 全局 CORS 响应头：公开端点（/config /healthz / 静态资源）与 API 统一生效，
/// 供跨主机手动连接场景读取响应（鉴权仍由 API 子路由的 auth_middleware 负责）。
async fn cors_response(response: Response<Body>) -> Response<Body> {
    let mut response = response;
    let headers = response.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        "authorization, content-type".parse().unwrap(),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        "GET, POST, OPTIONS".parse().unwrap(),
    );
    response
}

/// 构建路由：HTTP API 统一挂 `/api` 前缀（公开端点 + RPC / SSE / WebSocket），
/// 便于反向代理一条规则（如 vite dev proxy 的 `/api` → 后端）与前端客户端统一拼路径；
/// 其余路径为静态资源 / SPA history 路由。
///
/// 鉴权中间件只保护 API 子路由（route_layer），公开端点与静态资源免鉴权
/// （否则浏览器首屏拿不到 JS/CSS，同源自动发现也无法进行）。
/// feature `embed-static` 开启时，`fallback_service` 以 SPA 方式托管内嵌前端静态资源。
pub fn router(state: NetState) -> Router {
    let public = Router::new()
        .route("/api/healthz", get(|| async { "ok" }))
        .route("/api/config", get(handle_config));

    let api = Router::new()
        .route("/api/rpc", post(rpc::handle_rpc))
        .route("/api/events", get(sse::handle_sse))
        .route("/api/ws", get(ws::handle_ws))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ));

    #[cfg(feature = "embed-static")]
    {
        let spa = Router::new().fallback(static_assets::handle_spa);
        Router::new()
            .merge(public)
            .merge(api)
            .fallback_service(spa)
            .with_state(state)
            .layer(middleware::map_response(cors_response))
    }
    #[cfg(not(feature = "embed-static"))]
    {
        Router::new()
            .merge(public)
            .merge(api)
            .with_state(state)
            .layer(middleware::map_response(cors_response))
    }
}

/// 绑定并启动内嵌 server（错误记录后由调用方决定是否回退）。
pub async fn run_server(cfg: ServerConfig, state: NetState) -> Result<(), String> {
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("bind {addr} failed: {e}"))?;
    tracing::info!(
        addr = %addr,
        token_count = cfg.tokens.len(),
        event = STATE_CHANGED_EVENT,
        "network server listening (remote mode)"
    );
    axum::serve(listener, router(state))
        .await
        .map_err(|e| format!("network server error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::conversation_store::ConversationStore;
    use axum::{
        body::{to_bytes, Body},
        http::{header, Method, Request, StatusCode},
    };
    use base64::Engine;
    use futures_util::{SinkExt, StreamExt};
    use std::{
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tower::ServiceExt;

    /// 构造最小可服务 NetState（真实 Gateway + 临时目录 + 空 StateEmitter + 独立终端会话）。
    fn test_state(tokens: Vec<String>) -> NetState {
        let dir = std::env::temp_dir().join(format!(
            "pulsar-net-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let store = ConversationStore::new(&dir).expect("temp conversation store");
        let gateway = Gateway::new(store).expect("gateway");
        let state_emit: StateEmitter = Arc::new(|_| {});
        let (events_tx, _) = broadcast::channel::<StateChange>(16);
        let terminal = Arc::new(TerminalManager::new());
        let terminal_hub = TerminalEventHub::new_for_test();
        NetState {
            gateway,
            state_emit,
            events_tx,
            tokens,
            terminal,
            terminal_hub,
            host: "127.0.0.1".into(),
            port: 9999,
        }
    }

    async fn request(app: &Router, req: Request<Body>) -> axum::response::Response {
        app.clone()
            .oneshot(req)
            .await
            .expect("router serves request")
    }

    fn rpc_post(auth: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri("/api/rpc")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(token) = auth {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        builder
            .body(Body::from(r#"{"cmd":"debug_storage_path"}"#))
            .expect("valid rpc request")
    }

    #[tokio::test]
    async fn healthz_ok() {
        let app = router(test_state(vec![]));
        let res = request(
            &app,
            Request::builder()
                .uri("/api/healthz")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 1024).await.expect("read body");
        assert_eq!(&body[..], b"ok");
    }

    /// `/api/config` 公开端点：免鉴权返回运行信息（即使配了 token 也不 401）。
    #[tokio::test]
    async fn config_endpoint_public_and_informative() {
        let app = router(test_state(vec!["s3cret".into()]));
        let res = request(
            &app,
            Request::builder()
                .uri("/api/config")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 4096).await.expect("read body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
        assert_eq!(json["port"], 9999);
        assert_eq!(json["host"], "127.0.0.1");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["auth_required"], true);
        assert!(json["version"].is_string());
    }

    /// `/api/config` 公开端点：未配 token 时 auth_required=false。
    #[tokio::test]
    async fn config_endpoint_reports_no_auth_when_whitelist_empty() {
        let app = router(test_state(vec![]));
        let res = request(
            &app,
            Request::builder()
                .uri("/api/config")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 4096).await.expect("read body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
        assert_eq!(json["auth_required"], false);
    }

    #[tokio::test]
    async fn rpc_allowed_without_token_when_whitelist_empty() {
        let app = router(test_state(vec![]));
        let res = request(&app, rpc_post(None)).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 4096).await.expect("read body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
        assert_eq!(json["ok"], true);
        assert!(json["data"].is_string());
    }

    #[tokio::test]
    async fn rpc_rejects_missing_token() {
        let app = router(test_state(vec!["s3cret".into()]));
        let res = request(&app, rpc_post(None)).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rpc_accepts_header_token() {
        let app = router(test_state(vec!["s3cret".into()]));
        let res = request(&app, rpc_post(Some("s3cret"))).await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rpc_rejects_wrong_header_token() {
        let app = router(test_state(vec!["s3cret".into()]));
        let res = request(&app, rpc_post(Some("wrong"))).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn events_accepts_query_token() {
        // EventSource 无法带自定义头，token 走 query 参数（对齐 httpClient）。
        let app = router(test_state(vec!["s3cret".into()]));
        let res = request(
            &app,
            Request::builder()
                .uri("/api/events?token=s3cret")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let content_type = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .expect("content-type");
        assert!(
            content_type.starts_with("text/event-stream"),
            "expected SSE content-type, got {content_type}"
        );
    }

    #[tokio::test]
    async fn events_rejects_wrong_query_token() {
        let app = router(test_state(vec!["s3cret".into()]));
        let res = request(
            &app,
            Request::builder()
                .uri("/api/events?token=wrong")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn cors_preflight_passthrough() {
        let app = router(test_state(vec!["s3cret".into()]));
        let res = request(
            &app,
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/rpc")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            res.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "*"
        );
    }

    /// 读下一帧（超时 panic），解析为 JSON。
    async fn next_frame<S>(
        source: &mut S,
        deadline: std::time::Instant,
    ) -> serde_json::Value
    where
        S: futures_util::Stream<Item = Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>>
            + Unpin,
    {
        let remaining = deadline - std::time::Instant::now();
        let frame = tokio::time::timeout(remaining, source.next())
            .await
            .expect("frame within deadline")
            .expect("stream alive")
            .expect("text frame");
        serde_json::from_str(frame.to_text().unwrap()).expect("valid json frame")
    }

    /// 起真实 axum server（临时端口），返回 base 地址。
    async fn serve_app(state: NetState) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router(state)).await.unwrap();
        });
        format!("ws://{addr}")
    }

    /// /ws 端到端：带 topic 信封的 spawn → spawned → write(echo) → output → kill → exit 全链路。
    #[tokio::test]
    async fn ws_terminal_roundtrip_spawn_write_output_kill_exit() {
        use tokio_tungstenite::tungstenite::Message;

        let base = serve_app(test_state(vec![])).await;
        let (ws, _resp) = tokio_tungstenite::connect_async(format!("{base}/api/ws"))
            .await
            .expect("ws connect should succeed");
        let (mut sink, mut source) = ws.split();

        // 1) spawn（帧带 topic 信封）
        sink.send(
            Message::Text(r#"{"topic":"terminal","type":"spawn","shell":"sh"}"#.into()),
        )
        .await
        .unwrap();
        let spawned =
            next_frame(&mut source, std::time::Instant::now() + std::time::Duration::from_secs(3))
                .await;
        assert_eq!(spawned["topic"], "terminal");
        assert_eq!(spawned["type"], "spawned");
        let session_id = spawned["sessionId"].as_str().unwrap().to_string();

        // 2) write：echo 一行文本
        let data = base64::engine::general_purpose::STANDARD.encode(b"echo ws-roundtrip\n");
        sink.send(
            Message::Text(
                serde_json::json!({
                    "topic": "terminal",
                    "type": "write",
                    "sessionId": session_id,
                    "data": data,
                })
                .to_string()
                .into(),
            ),
        )
        .await
        .unwrap();

        // 3) 收输出帧，直到看到 echo 内容
        let mut saw_output = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !saw_output {
            let value = next_frame(&mut source, deadline).await;
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
                serde_json::json!({
                    "topic": "terminal",
                    "type": "kill",
                    "sessionId": session_id,
                })
                .to_string()
                .into(),
            ),
        )
        .await
        .unwrap();
        let mut saw_exit = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !saw_exit {
            let value = next_frame(&mut source, deadline).await;
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
    }

    /// token 白名单非空时，无 token 的 WS 握手应被拒绝（401）。
    #[tokio::test]
    async fn ws_handshake_requires_token_when_whitelist_nonempty() {
        let base = serve_app(test_state(vec!["s3cret".into()])).await;
        let err = tokio_tungstenite::connect_async(format!("{base}/api/ws"))
            .await
            .expect_err("no-token handshake should fail");
        assert!(
            err.to_string().contains("401") || err.to_string().contains("Unauthorized"),
            "expected 401 rejection, got: {err}"
        );
    }

    /// token 白名单非空时，带 `?token=` 的 WS 握手成功并可完成一次 list。
    #[tokio::test]
    async fn ws_handshake_accepts_query_token() {
        use tokio_tungstenite::tungstenite::Message;

        let base = serve_app(test_state(vec!["s3cret".into()])).await;
        let (ws, _resp) =
            tokio_tungstenite::connect_async(format!("{base}/api/ws?token=s3cret"))
                .await
                .expect("token handshake should succeed");
        let (mut sink, mut source) = ws.split();
        sink.send(Message::Text(r#"{"topic":"terminal","type":"list"}"#.into()))
            .await
            .unwrap();
        let value = next_frame(&mut source, std::time::Instant::now() + std::time::Duration::from_secs(3))
            .await;
        assert_eq!(value["topic"], "terminal");
        assert_eq!(value["type"], "list");
        assert!(value["sessions"].is_array());
    }
}

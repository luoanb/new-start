//! 内嵌网络服务（远程模式）。
//!
//! 在 Tauri 进程内按 `config.json` 顶层 `server` 节条件启动 axum HTTP server，
//! 复用 `Gateway` 与分域 State，把 54 个 Tauri command 以统一 RPC 端点暴露，
//! 并把 `StateChange` 通过 SSE 推送给远程前端。本机 Tauri IPC 路径不受影响。

pub mod auth;
pub mod rpc;
pub mod sse;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tokio::sync::broadcast;

use crate::core::{events::STATE_CHANGED_EVENT, Gateway, StateChange, StateEmitter};

/// 内嵌 server 运行配置（源自 `config.json` `server` 节，缺省不启动）。
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub tokens: Vec<String>,
}

/// axum managed state：可 Clone 的 `Gateway` + 状态发射器 + SSE 广播通道 + token 白名单。
#[derive(Clone)]
pub struct NetState {
    pub gateway: Gateway,
    pub state_emit: StateEmitter,
    pub events_tx: broadcast::Sender<StateChange>,
    pub tokens: Vec<String>,
}

/// 构建路由：RPC 端点 / SSE 事件流 / 存活检查。
pub fn router(state: NetState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/rpc", post(rpc::handle_rpc))
        .route("/events", get(sse::handle_sse))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        .with_state(state)
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
    use std::{
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tower::ServiceExt;

    /// 构造最小可服务 NetState（真实 Gateway + 临时目录 + 空 StateEmitter）。
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
        NetState {
            gateway,
            state_emit,
            events_tx,
            tokens,
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
            .uri("/rpc")
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
                .uri("/healthz")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 1024).await.expect("read body");
        assert_eq!(&body[..], b"ok");
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
                .uri("/events?token=s3cret")
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
                .uri("/events?token=wrong")
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
                .uri("/rpc")
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
}

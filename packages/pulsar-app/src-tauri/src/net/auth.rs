//! 网络服务鉴权与 CORS 中间件。
//!
//! 鉴权规则（对齐 micro_spec `2026-08-14_21-13_network-remote-mode.md`）：
//! - token 白名单为空 → 放行（默认监听 loopback，仅本机可达，等价本机免鉴权）。
//! - token 白名单非空 → 所有请求（含本机）必须携带 `Authorization: Bearer <token>`，
//!   且 token 属于白名单，否则返回 401。

use axum::{
    body::Body,
    extract::State,
    http::{header, Method, Request, Response, StatusCode},
    middleware::Next,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use super::NetState;

fn with_cors(mut response: Response<Body>) -> Response<Body> {
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

pub async fn auth_middleware(
    State(state): State<NetState>,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    // CORS 预检：直接放行。
    if req.method() == Method::OPTIONS {
        return with_cors(
            Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body(axum::body::Body::empty())
                .expect("static 204 response"),
        );
    }

    if !state.tokens.is_empty() {
        // token 可来自 Authorization header（POST /rpc）或 query 参数 `?token=`（EventSource 无法带 header）。
        let authorized = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .or_else(|| query_token(&req))
            .map(|token| state.tokens.iter().any(|t| t == token))
            .unwrap_or(false);
        if !authorized {
            let body = Json(json!({
                "ok": false,
                "error": {
                    "code": "unauthorized",
                    "message": "missing or invalid bearer token",
                },
            }));
            return with_cors(
                (StatusCode::UNAUTHORIZED, body)
                    .into_response(),
            );
        }
    }

    with_cors(next.run(req).await)
}

/// 从 `?token=` 解析 token（简单 split，不做 percent-decode；token 建议用字母数字）。
fn query_token(req: &Request<Body>) -> Option<&str> {
    req.uri().query().and_then(|query| {
        query.split('&').find_map(|pair| {
            let mut it = pair.split('=');
            if it.next() == Some("token") {
                it.next()
            } else {
                None
            }
        })
    })
}

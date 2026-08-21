//! 前端静态资源托管（feature `embed-static`）。
//!
//! 用 rust-embed 把 SvelteKit 构建产物 `build/` 编译进二进制，由内嵌 server 以
//! SPA 方式对外服务：按路径返回文件，未命中回退 `index.html`（history 路由）。
//! 该副本与 Tauri WebView 内嵌资源相互独立，仅服务于远程访问路径。

use axum::{
    body::Body,
    http::{header, Uri, StatusCode},
    response::Response,
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../build/"]
struct FrontendAssets;

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("json") | Some("map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
}

/// SPA fallback handler：命中资源按其扩展名返回；未命中时，
/// 带扩展名的资源请求返回 404（避免 HTML 冒充静态资源），
/// 无扩展名视为 history 路由回退 index.html（按 HTML 返回，防止浏览器当成文件下载）。
/// HTTP API 统一挂 `/api` 前缀，未命中该前缀的 API 请求一律 404，绝不回退 index.html。
pub async fn handle_spa(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return not_found();
    }
    if let Some(file) = FrontendAssets::get(path) {
        return serve(path, file);
    }
    if path.rsplit('/').next().is_some_and(|seg| seg.contains('.')) {
        return not_found();
    }
    let file = FrontendAssets::get("index.html").expect("build/index.html exists");
    serve("index.html", file)
}

fn serve(path: &str, file: rust_embed::EmbeddedFile) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type(path))
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(file.data.into_owned()))
        .expect("valid static response")
}

fn not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("not found"))
        .expect("valid 404 response")
}

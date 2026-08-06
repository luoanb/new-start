//! MCP（Model Context Protocol）客户端适配。
//!
//! 纯 A 语义：server 连接在启动期一次性装配（per-server 超时，失败 warn + skip，
//! 不阻塞应用启动）；无会话中热增 / 热重扫，改配置需重启生效。
//!
//! 传输支持：
//! - `stdio`：spawn 子进程（`TokioChildProcess`），子进程 stderr 继承应用 stderr。
//! - `http`：streamable-http（`StreamableHttpClientTransport` + reqwest）。
//!
//! 连接生命周期由 `McpServerClient` 持有，drop 时由 rmcp 关闭连接 / 清理子进程。

use std::{collections::HashMap, process::Stdio, sync::Arc, time::Duration};

use async_trait::async_trait;
use http::{HeaderName, HeaderValue};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ClientCapabilities, ClientInfo,
    ContentBlock, Implementation,
};
use rmcp::service::{Peer, RoleClient, RunningService, ServiceExt};
use rmcp::transport::{
    streamable_http_client::StreamableHttpClientTransportConfig, IntoTransport,
    StreamableHttpClientTransport, TokioChildProcess,
};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use super::error::{AppError, AppResult};
use super::tool_config::McpServerConfig;
use super::tool_registry::Tool;

/// 单 server 装配期连接超时。
const CONNECT_TIMEOUT_MS: u64 = 15_000;
/// 单次 tools/call 超时。
const CALL_TIMEOUT_MS: u64 = 120_000;
/// tools/call 文本结果截断上限。
const MAX_RESULT_CHARS: usize = 64 * 1024;

/// MCP server 连接状态（前端只读面板展示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpServerStatusKind {
    Connected,
    Failed,
    Disabled,
}

/// 装配期收集的单个 MCP server 状态。
#[derive(Debug, Clone, Serialize)]
pub struct McpServerStatus {
    pub name: String,
    pub transport: String,
    pub status: McpServerStatusKind,
    pub tool_count: usize,
    pub error: Option<String>,
}

/// 客户端标识：agent-app。
#[derive(Debug)]
struct AgentClientHandler;

impl rmcp::ClientHandler for AgentClientHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("agent-app", env!("CARGO_PKG_VERSION")),
        )
    }
}

/// 一个 MCP server 的装配期连接封装。
pub struct McpServerClient {
    name: String,
    transport: String,
    peer: Peer<RoleClient>,
    /// 保活连接：drop 时由 rmcp 关闭连接 / 清理子进程。
    _keepalive: Mutex<RunningService<RoleClient, AgentClientHandler>>,
}

impl McpServerClient {
    /// 根据配置连接 MCP server（stdio / http），成功返回 `Arc<Self>` 供工具共享。
    pub async fn connect(cfg: McpServerConfig) -> AppResult<Arc<Self>> {
        let name = cfg.name.clone();
        let transport = cfg.transport.clone();
        let running = match transport.as_str() {
            "stdio" => connect_stdio(&cfg).await?,
            "http" => connect_http(&cfg).await?,
            other => {
                return Err(AppError::InvalidInput(format!(
                    "mcp[{name}]: 不支持的传输类型 {other:?}（仅支持 stdio / http）"
                )))
            }
        };
        Ok(Arc::new(Self::from_running(name, transport, running).await?))
    }

    /// 内部：从已建立的连接构造 client（测试可注入自定义 transport）。
    async fn from_running(
        name: String,
        transport: String,
        running: RunningService<RoleClient, AgentClientHandler>,
    ) -> AppResult<Self> {
        let peer = running.peer().clone();
        Ok(Self {
            name,
            transport,
            peer,
            _keepalive: Mutex::new(running),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn transport(&self) -> &str {
        &self.transport
    }

    /// `tools/list` → 本 server 暴露的工具定义列表（含 `tools/call` 转发）。
    pub async fn discover_tools(self: &Arc<Self>) -> AppResult<Vec<McpTool>> {
        let tools = self.peer.list_all_tools().await.map_err(|e| {
            AppError::RuntimeError(format!("mcp[{}]: tools/list 失败: {e}", self.name))
        })?;
        let mut out = Vec::with_capacity(tools.len());
        for t in tools {
            let description = t.description.as_deref().unwrap_or("").to_string();
            if description.trim().is_empty() {
                tracing::warn!(
                    server = self.name.as_str(),
                    tool = t.name.as_ref(),
                    "mcp tool has empty description; model may misuse it"
                );
            }
            let parameters = Value::Object((*t.input_schema).clone());
            out.push(McpTool::new(
                Arc::clone(self),
                t.name.to_string(),
                description,
                parameters,
            ));
        }
        Ok(out)
    }

    /// `tools/call` 转发；结果统一渲染为字符串返回给 Agent 循环。
    async fn call_tool(&self, tool_name: &str, args: Value) -> AppResult<String> {
        let arguments = args.as_object().cloned().unwrap_or_default();
        let params = CallToolRequestParams::new(tool_name.to_string()).with_arguments(arguments);
        let result = tokio::time::timeout(
            Duration::from_millis(CALL_TIMEOUT_MS),
            self.peer.call_tool_once(params),
        )
        .await
        .map_err(|_elapsed| {
            AppError::RuntimeError(format!(
                "mcp[{}]: 工具 {tool_name} 调用超时（{}ms）",
                self.name, CALL_TIMEOUT_MS
            ))
        })?
        .map_err(|e| {
            AppError::RuntimeError(format!("mcp[{}]: 工具 {tool_name} 调用失败: {e}", self.name))
        })?;

        match result {
            CallToolResponse::Complete(res) => render_result(tool_name, &res),
            _ => Err(AppError::RuntimeError(format!(
                "mcp[{}]: 工具 {tool_name} 返回未完成结果",
                self.name
            ))),
        }
    }
}

/// MCP server 暴露的单个工具适配：`execute` 转发 `tools/call`。
pub struct McpTool {
    server_name: String,
    name: String,
    description: String,
    parameters: Value,
    client: Arc<McpServerClient>,
}

impl McpTool {
    fn new(
        client: Arc<McpServerClient>,
        name: String,
        description: String,
        parameters: Value,
    ) -> Self {
        Self {
            server_name: client.name().to_string(),
            name,
            description,
            parameters,
            client,
        }
    }

    /// 工具来源归属（仅日志 / 调试用）。
    pub fn server_name(&self) -> &str {
        &self.server_name
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        self.parameters.clone()
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        self.client.call_tool(&self.name, args).await
    }
}

/// stdio 传输：spawn 子进程并以 stdio 交换 JSON-RPC。
async fn connect_stdio(
    cfg: &McpServerConfig,
) -> AppResult<RunningService<RoleClient, AgentClientHandler>> {
    let command = cfg.command.as_ref().ok_or_else(|| {
        AppError::InvalidInput(format!("mcp[{}]: stdio 传输缺少 command", cfg.name))
    })?;
    let mut cmd = tokio::process::Command::new(command);
    cmd.args(&cfg.args);
    cmd.envs(&cfg.env);
    let (transport, _stderr) = TokioChildProcess::builder(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            AppError::RuntimeError(format!("mcp[{}]: 启动 stdio server 失败: {e}", cfg.name))
        })?;
    serve_with_timeout(cfg, transport).await
}

/// streamable-http 传输：具名端点 + 可选自定义请求头。
async fn connect_http(
    cfg: &McpServerConfig,
) -> AppResult<RunningService<RoleClient, AgentClientHandler>> {
    let url = cfg.url.as_ref().ok_or_else(|| {
        AppError::InvalidInput(format!("mcp[{}]: http 传输缺少 url", cfg.name))
    })?;
    let mut headers: HashMap<HeaderName, HeaderValue> = HashMap::new();
    for (key, value) in &cfg.headers {
        let name = HeaderName::from_bytes(key.as_bytes()).map_err(|e| {
            AppError::InvalidInput(format!("mcp[{}]: 非法 header 名 {key:?}: {e}", cfg.name))
        })?;
        let value = HeaderValue::from_str(value).map_err(|e| {
            AppError::InvalidInput(format!("mcp[{}]: 非法 header 值: {e}", cfg.name))
        })?;
        headers.insert(name, value);
    }
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(url.as_str()).custom_headers(headers),
    );
    serve_with_timeout(cfg, transport).await
}

/// 连接握手统一加超时：失败返回可读错误（装配方决定 warn + skip）。
async fn serve_with_timeout<T, E, A>(
    cfg: &McpServerConfig,
    transport: T,
) -> AppResult<RunningService<RoleClient, AgentClientHandler>>
where
    T: IntoTransport<RoleClient, E, A>,
    E: std::error::Error + Send + Sync + 'static,
    A: 'static,
{
    tokio::time::timeout(
        Duration::from_millis(CONNECT_TIMEOUT_MS),
        AgentClientHandler.serve(transport),
    )
    .await
    .map_err(|_elapsed| {
        AppError::RuntimeError(format!("mcp[{}]: 连接超时", cfg.name))
    })?
    .map_err(|e| AppError::RuntimeError(format!("mcp[{}]: 连接失败: {e}", cfg.name)))
}

/// 将 `tools/call` 结果渲染为字符串；结构化结果优先。
fn render_result(tool_name: &str, res: &CallToolResult) -> AppResult<String> {
    if let Some(sc) = &res.structured_content {
        return Ok(truncate_text(&sc.to_string(), MAX_RESULT_CHARS));
    }
    let text = blocks_to_text(&res.content);
    if res.is_error == Some(true) {
        return Ok(json!({
            "is_error": true,
            "content": truncate_text(&text, MAX_RESULT_CHARS)
        })
        .to_string());
    }
    let _ = tool_name;
    Ok(truncate_text(&text, MAX_RESULT_CHARS))
}

fn blocks_to_text(blocks: &[ContentBlock]) -> String {
    let mut parts = Vec::new();
    for b in blocks {
        parts.push(match b {
            ContentBlock::Text(t) => t.text.clone(),
            ContentBlock::Image(_) => "[image content]".to_string(),
            ContentBlock::Audio(_) => "[audio content]".to_string(),
            ContentBlock::Resource(_) => "[embedded resource]".to_string(),
            ContentBlock::ResourceLink(r) => format!("[resource link: {}]", r.name),
            _ => "[unsupported content block]".to_string(),
        });
    }
    parts.join("\n")
}

fn truncate_text(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}\n[truncated: result exceeds {max} chars]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tool_registry::Tool;
    use rmcp::model::{ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool as McpModelTool};
    use rmcp::service::{RequestContext, RoleServer};
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };
    use rmcp::{ErrorData as McpError, ServerHandler};

    /// 最小 MCP mock server：暴露 `mock_echo` 工具，返回 `echo:{text}`。
    #[derive(Debug, Default)]
    struct MockMcpServer;

    impl ServerHandler for MockMcpServer {
        fn get_info(&self) -> ServerInfo {
            ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
        }

        async fn list_tools(
            &self,
            _request: Option<PaginatedRequestParams>,
            _context: RequestContext<RoleServer>,
        ) -> Result<ListToolsResult, McpError> {
            let schema = json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            });
            let tool = McpModelTool::new(
                "mock_echo",
                "Mock echo tool",
                schema.as_object().cloned().unwrap(),
            );
            Ok(ListToolsResult::with_all_items(vec![tool]))
        }

        async fn call_tool(
            &self,
            request: CallToolRequestParams,
            _context: RequestContext<RoleServer>,
        ) -> Result<CallToolResponse, McpError> {
            let text = request
                .arguments
                .as_ref()
                .and_then(|m| m.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Ok(CallToolResponse::Complete(CallToolResult::success(vec![
                ContentBlock::text(format!("echo:{text}")),
            ])))
        }
    }

    /// 通过 in-memory duplex 模拟 stdio 全链路：连接 → tools/list → tools/call。
    #[tokio::test]
    async fn discover_and_call_over_duplex_stdio() {
        let (server_transport, client_transport) = tokio::io::duplex(4096);

        let server_handle = tokio::spawn(async move {
            let running = MockMcpServer
                .serve(server_transport)
                .await
                .expect("server handshake");
            // 保活至连接关闭；duplex 下 client drop 不产生 EOF，测试结束由 abort 清理。
            let _ = running.waiting().await;
        });

        let running = AgentClientHandler
            .serve(client_transport)
            .await
            .expect("client handshake");
        let client = Arc::new(
            McpServerClient::from_running("mock".into(), "stdio".into(), running)
                .await
                .expect("construct client"),
        );

        let tools = client.discover_tools().await.expect("discover tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "mock_echo");
        assert_eq!(tools[0].description(), "Mock echo tool");
        assert_eq!(tools[0].server_name(), "mock");

        let out = client
            .call_tool("mock_echo", json!({ "text": "hi" }))
            .await
            .expect("call tool");
        assert_eq!(out, "echo:hi");

        drop(client);
        server_handle.abort();
    }

    /// streamable-http 全链路：axum 本地 mock server → connect_http → tools/list → tools/call。
    #[tokio::test]
    async fn discover_and_call_over_streamable_http() {
        let service: StreamableHttpService<MockMcpServer, LocalSessionManager> =
            StreamableHttpService::new(
                || Ok(MockMcpServer::default()),
                Default::default(),
                StreamableHttpServerConfig::default(),
            );
        let router = axum::Router::new().nest_service("/mcp", service);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let addr = listener.local_addr().expect("local addr");

        let server_handle = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let cfg = McpServerConfig {
            name: "http-mock".into(),
            transport: "http".into(),
            command: None,
            args: vec![],
            env: HashMap::new(),
            url: Some(format!("http://{addr}/mcp")),
            headers: HashMap::new(),
            disabled: false,
        };
        let client = McpServerClient::connect(cfg).await.expect("http connect");
        let tools = client.discover_tools().await.expect("discover tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "mock_echo");

        let out = client
            .call_tool("mock_echo", json!({ "text": "hi" }))
            .await
            .expect("call tool");
        assert_eq!(out, "echo:hi");

        drop(client);
        server_handle.abort();
    }

    /// 连接不可达端点应失败（装配方负责 warn + skip，不阻塞启动）。
    #[tokio::test]
    async fn connect_http_to_unreachable_port_fails() {
        let cfg = McpServerConfig {
            name: "unreachable".into(),
            transport: "http".into(),
            command: None,
            args: vec![],
            env: HashMap::new(),
            url: Some("http://127.0.0.1:1/mcp".into()),
            headers: HashMap::new(),
            disabled: false,
        };
        let err = match McpServerClient::connect(cfg).await {
            Ok(_) => panic!("expected connection failure"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("unreachable"));
    }

    #[test]
    fn truncate_text_shortens_and_marks() {
        assert_eq!(truncate_text("ok", 10), "ok");
        let long = "a".repeat(100);
        let out = truncate_text(&long, 10);
        assert!(out.contains("[truncated"));
        assert_eq!(out.chars().count(), 10 + 1 + "[truncated: result exceeds 10 chars]".len());
    }

    #[test]
    fn blocks_to_text_renders_text_and_marks_others() {
        let blocks = vec![
            ContentBlock::text("hello"),
            ContentBlock::image("AA==", "image/png"),
        ];
        let text = blocks_to_text(&blocks);
        assert!(text.contains("hello"));
        assert!(text.contains("[image content]"));
    }

    #[tokio::test]
    async fn connect_with_unsupported_transport_errors() {
        let cfg = McpServerConfig {
            name: "bad".into(),
            transport: "ws".into(),
            command: None,
            args: vec![],
            env: HashMap::new(),
            url: None,
            headers: HashMap::new(),
            disabled: false,
        };
        let err = match McpServerClient::connect(cfg).await {
            Ok(_) => panic!("expected error for unsupported transport"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("不支持的传输类型"));
    }
}

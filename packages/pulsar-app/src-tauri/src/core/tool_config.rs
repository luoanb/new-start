//! 工具运行期配置读写：`mcp_servers.json`（MCP server 声明）与 `dynamic_tools.json`
//! （配置驱动 DynamicTool 声明）。纯 A + 运行期手动重装配语义：启动期读取装配，
//! 运行期由 UI 保存（`save_*` 原子写回）后触发全量重装配。
//!
//! 读取范式仿 `NeuronConfigReader`：文件缺失返回默认空；非法配置返回可读错误，
//! 由调用方（gateway 装配 / 保存校验）决定 warn + skip 或拒绝保存。

use crate::core::cmd_exec;
use crate::core::error::{AppError, AppResult};
use crate::core::models::ToolTag;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

/// 单个 MCP server 配置（`mcp_servers.json` 条目）。
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct McpServerConfig {
    pub name: String,
    /// `"stdio"` 或 `"http"`。
    pub transport: String,
    /// stdio 传输：可执行文件或包命令（如 `npx`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
    /// http 传输：streamable-http 端点 URL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// 附加请求头（http 传输）。
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    /// 显式禁用该 server（保留配置但不启用）。
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
    /// 工具标签：该 server 下全部工具打此标（面板注册可指定），缺省 normal。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<ToolTag>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct McpServersFile {
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
}

/// 配置驱动 HTTP 工具（具名固定端点）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpToolConfig {
    pub name: String,
    pub desc: String,
    /// HTTP 方法，默认 GET。
    #[serde(default = "default_http_method")]
    pub method: String,
    /// 含 `{param}` 占位符的完整 URL；端点固定，模型只能填参数值。
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// 工具标签（面板注册可指定），缺省 normal。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<ToolTag>,
}

/// 配置驱动命令模板工具。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandToolConfig {
    pub name: String,
    pub desc: String,
    /// 含 `{param}` 占位符的命令模板；最终命令必须通过 cmd_exec 安全护栏。
    pub template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// 工具标签（面板注册可指定），缺省 normal。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<ToolTag>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DynamicToolsFile {
    #[serde(default)]
    pub http: Vec<HttpToolConfig>,
    #[serde(default)]
    pub command: Vec<CommandToolConfig>,
}

/// 前后端共享的工具配置视图（弹窗编辑与写回的单一数据形状）。
/// 前端字段名与后端 serde 键保持一致。
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ToolConfigView {
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub http_tools: Vec<HttpToolConfig>,
    #[serde(default)]
    pub command_tools: Vec<CommandToolConfig>,
}

fn default_http_method() -> String {
    "GET".to_string()
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// 启动期工具配置读取器（纯 A：无热更新）。
pub struct ToolConfigReader {
    storage_root: PathBuf,
}

impl ToolConfigReader {
    pub fn new(storage_root: impl Into<PathBuf>) -> Self {
        Self {
            storage_root: storage_root.into(),
        }
    }

    pub fn mcp_servers(&self) -> AppResult<McpServersFile> {
        self.read_json("mcp_servers.json")
    }

    pub fn dynamic_tools(&self) -> AppResult<DynamicToolsFile> {
        self.read_json("dynamic_tools.json")
    }

    /// 原子写回 `mcp_servers.json`（临时文件 + rename）。
    pub fn save_mcp_servers(&self, file: &McpServersFile) -> AppResult<()> {
        self.write_json("mcp_servers.json", file)
    }

    /// 原子写回 `dynamic_tools.json`（临时文件 + rename）。
    pub fn save_dynamic_tools(&self, file: &DynamicToolsFile) -> AppResult<()> {
        self.write_json("dynamic_tools.json", file)
    }

    fn read_json<T: for<'de> Deserialize<'de> + Default>(&self, file: &str) -> AppResult<T> {
        let path = self.storage_root.join(file);
        if !path.exists() {
            return Ok(T::default());
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| AppError::RuntimeError(format!("读取 {} 失败: {e}", path.display())))?;
        serde_json::from_str(&content)
            .map_err(|e| AppError::RuntimeError(format!("解析 {} 失败: {e}", path.display())))
    }

    /// 原子写回：写入临时文件后 rename，避免写一半的 JSON 被装配读到。
    fn write_json<T: Serialize>(&self, file: &str, value: &T) -> AppResult<()> {
        let path = self.storage_root.join(file);
        fs::create_dir_all(&self.storage_root).map_err(|_| {
            AppError::RuntimeError(format!("创建配置目录失败: {}", self.storage_root.display()))
        })?;
        let content = serde_json::to_string_pretty(value)
            .map_err(|e| AppError::RuntimeError(format!("序列化 {file} 失败: {e}")))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, content)
            .map_err(|e| AppError::RuntimeError(format!("写入 {} 失败: {e}", tmp.display())))?;
        fs::rename(&tmp, &path)
            .map_err(|e| AppError::RuntimeError(format!("替换 {} 失败: {e}", path.display())))?;
        Ok(())
    }
}

/// 保存前校验：非法配置拒绝写回与触发重装配，返回可读错误。
pub fn validate_tool_config(view: &ToolConfigView) -> AppResult<()> {
    let mut mcp_names = HashSet::new();
    for server in &view.mcp_servers {
        if server.name.trim().is_empty() {
            return Err(AppError::InvalidInput("MCP server 的 name 不能为空".into()));
        }
        if !mcp_names.insert(server.name.clone()) {
            return Err(AppError::InvalidInput(format!(
                "MCP server name 重复: {}",
                server.name
            )));
        }
        match server.transport.as_str() {
            "stdio" => {
                if server.command.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(AppError::InvalidInput(format!(
                        "MCP server {}（stdio）缺少 command",
                        server.name
                    )));
                }
            }
            "http" => {
                if server.url.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(AppError::InvalidInput(format!(
                        "MCP server {}（http）缺少 url",
                        server.name
                    )));
                }
            }
            other => {
                return Err(AppError::InvalidInput(format!(
                    "MCP server {} 未知 transport: {}（仅支持 stdio / http）",
                    server.name, other
                )));
            }
        }
    }

    let mut http_names = HashSet::new();
    for tool in &view.http_tools {
        if tool.name.trim().is_empty() {
            return Err(AppError::InvalidInput("HTTP tool 的 name 不能为空".into()));
        }
        if !http_names.insert(tool.name.clone()) {
            return Err(AppError::InvalidInput(format!(
                "HTTP tool name 重复: {}",
                tool.name
            )));
        }
        if tool.url.trim().is_empty() {
            return Err(AppError::InvalidInput(format!(
                "HTTP tool {} 缺少 url",
                tool.name
            )));
        }
        let method = tool.method.to_uppercase();
        if !["GET", "POST", "PUT", "DELETE"].contains(&method.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "HTTP tool {} 非法 method: {}（仅支持 GET/POST/PUT/DELETE）",
                tool.name, tool.method
            )));
        }
    }

    let mut cmd_names = HashSet::new();
    for tool in &view.command_tools {
        if tool.name.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Command tool 的 name 不能为空".into(),
            ));
        }
        if !cmd_names.insert(tool.name.clone()) {
            return Err(AppError::InvalidInput(format!(
                "Command tool name 重复: {}",
                tool.name
            )));
        }
        if tool.template.trim().is_empty() {
            return Err(AppError::InvalidInput(format!(
                "Command tool {} 缺少 template",
                tool.name
            )));
        }
        if cmd_exec::is_denied(&tool.template) {
            return Err(AppError::InvalidInput(format!(
                "Command tool {} 模板被安全策略拒绝",
                tool.name
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn tmp_root() -> PathBuf {
        // 每个测试独立目录，避免并行执行时相互踩踏。
        let seq = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("tool-config-test-{}-{seq}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(root: &PathBuf, name: &str, content: &str) {
        let mut f = fs::File::create(root.join(name)).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn missing_files_yield_defaults() {
        let root = tmp_root();
        let reader = ToolConfigReader::new(&root);
        assert_eq!(reader.mcp_servers().unwrap().mcp_servers.len(), 0);
        assert_eq!(reader.dynamic_tools().unwrap().http.len(), 0);
        assert_eq!(reader.dynamic_tools().unwrap().command.len(), 0);
    }

    #[test]
    fn parses_mcp_servers_file() {
        let root = tmp_root();
        write_file(
            &root,
            "mcp_servers.json",
            r#"{
                "mcp_servers": [
                    {
                        "name": "filesystem",
                        "transport": "stdio",
                        "command": "npx",
                        "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
                        "disabled": false
                    },
                    {
                        "name": "docs",
                        "transport": "http",
                        "url": "http://127.0.0.1:8000/mcp",
                        "headers": { "Authorization": "Bearer x" }
                    }
                ]
            }"#,
        );
        let file = ToolConfigReader::new(&root).mcp_servers().unwrap();
        assert_eq!(file.mcp_servers.len(), 2);
        assert_eq!(file.mcp_servers[0].command.as_deref(), Some("npx"));
        assert_eq!(file.mcp_servers[1].transport, "http");
        assert_eq!(
            file.mcp_servers[1]
                .headers
                .get("Authorization")
                .map(String::as_str),
            Some("Bearer x")
        );
    }

    #[test]
    fn parses_dynamic_tools_file() {
        let root = tmp_root();
        write_file(
            &root,
            "dynamic_tools.json",
            r#"{
                "http": [
                    { "name": "lookup_wiki", "desc": "查内部 wiki", "method": "GET",
                      "url": "https://api.example.com/wiki?q={query}" }
                ],
                "command": [
                    { "name": "git_status", "desc": "查看 git 状态", "template": "git status --porcelain" }
                ]
            }"#,
        );
        let file = ToolConfigReader::new(&root).dynamic_tools().unwrap();
        assert_eq!(file.http.len(), 1);
        assert_eq!(file.http[0].name, "lookup_wiki");
        assert_eq!(file.http[0].method, "GET");
        assert_eq!(file.command.len(), 1);
        assert_eq!(file.command[0].template, "git status --porcelain");
    }

    #[test]
    fn parses_config_tag_field() {
        let root = tmp_root();
        // MCP server 与 DynamicTool 均可声明 tag；缺省为 None（→ Normal）。
        write_file(
            &root,
            "mcp_servers.json",
            r#"{
                "mcp_servers": [
                    { "name": "filesystem", "transport": "stdio", "command": "npx", "tag": "system" },
                    { "name": "docs", "transport": "http", "url": "http://127.0.0.1:8000/mcp" }
                ]
            }"#,
        );
        let file = ToolConfigReader::new(&root).mcp_servers().unwrap();
        assert_eq!(file.mcp_servers[0].tag, Some(ToolTag::System));
        assert_eq!(file.mcp_servers[1].tag, None, "缺省 tag 为 None（→ Normal）");

        write_file(
            &root,
            "dynamic_tools.json",
            r#"{
                "http": [
                    { "name": "lookup_wiki", "desc": "d", "method": "GET",
                      "url": "https://api.example.com/wiki?q={query}", "tag": "core" }
                ],
                "command": [
                    { "name": "git_status", "desc": "d", "template": "git status --porcelain" }
                ]
            }"#,
        );
        let file = ToolConfigReader::new(&root).dynamic_tools().unwrap();
        assert_eq!(file.http[0].tag, Some(ToolTag::Core));
        assert_eq!(file.command[0].tag, None);
    }

    #[test]
    fn invalid_json_returns_error() {
        let root = tmp_root();
        write_file(&root, "mcp_servers.json", "not json");
        assert!(ToolConfigReader::new(&root).mcp_servers().is_err());
    }

    fn sample_view() -> ToolConfigView {
        ToolConfigView {
            mcp_servers: vec![McpServerConfig {
                name: "filesystem".into(),
                transport: "stdio".into(),
                command: Some("npx".into()),
                args: vec![
                    "-y".into(),
                    "@modelcontextprotocol/server-filesystem".into(),
                ],
                env: Default::default(),
                url: None,
                headers: Default::default(),
                disabled: false,
                tag: None,
            }],
            http_tools: vec![HttpToolConfig {
                name: "lookup_wiki".into(),
                desc: "查内部 wiki".into(),
                method: "GET".into(),
                url: "https://api.example.com/wiki?q={query}".into(),
                timeout_ms: None,
                tag: None,
            }],
            command_tools: vec![CommandToolConfig {
                name: "git_status".into(),
                desc: "查看 git 状态".into(),
                template: "git status --porcelain".into(),
                timeout_ms: None,
                tag: None,
            }],
        }
    }

    #[test]
    fn save_mcp_servers_roundtrip() {
        let root = tmp_root();
        let reader = ToolConfigReader::new(&root);
        let view = sample_view();
        reader
            .save_mcp_servers(&McpServersFile {
                mcp_servers: view.mcp_servers.clone(),
            })
            .unwrap();
        let file = reader.mcp_servers().unwrap();
        assert_eq!(file.mcp_servers, view.mcp_servers);
        // 写回为可解析 JSON（原子写不残留临时文件）。
        assert!(root.join("mcp_servers.json").exists());
        assert!(!root.join("mcp_servers.json.tmp").exists());
    }

    #[test]
    fn save_dynamic_tools_roundtrip() {
        let root = tmp_root();
        let reader = ToolConfigReader::new(&root);
        let view = sample_view();
        reader
            .save_dynamic_tools(&DynamicToolsFile {
                http: view.http_tools.clone(),
                command: view.command_tools.clone(),
            })
            .unwrap();
        let file = reader.dynamic_tools().unwrap();
        assert_eq!(file.http.len(), 1);
        assert_eq!(file.http[0].name, "lookup_wiki");
        assert_eq!(file.http[0].method, "GET");
        assert_eq!(file.command[0].template, "git status --porcelain");
    }

    #[test]
    fn validate_accepts_valid_config() {
        assert!(validate_tool_config(&sample_view()).is_ok());
    }

    #[test]
    fn validate_rejects_unknown_transport() {
        let mut view = sample_view();
        view.mcp_servers[0].transport = "sse".into();
        let err = validate_tool_config(&view).unwrap_err();
        assert!(err.to_string().contains("未知 transport"));
    }

    #[test]
    fn validate_rejects_stdio_without_command() {
        let mut view = sample_view();
        view.mcp_servers[0].command = None;
        assert!(validate_tool_config(&view).is_err());
    }

    #[test]
    fn validate_rejects_http_without_url() {
        let mut view = sample_view();
        view.mcp_servers[0].transport = "http".into();
        view.mcp_servers[0].url = None;
        assert!(validate_tool_config(&view).is_err());
    }

    #[test]
    fn validate_rejects_duplicate_mcp_name() {
        let mut view = sample_view();
        view.mcp_servers.push(view.mcp_servers[0].clone());
        assert!(validate_tool_config(&view).is_err());
    }

    #[test]
    fn validate_rejects_denied_command_template() {
        let mut view = sample_view();
        view.command_tools[0].template = "sudo rm -rf /".into();
        assert!(validate_tool_config(&view).is_err());
    }

    #[test]
    fn validate_rejects_bad_http_method() {
        let mut view = sample_view();
        view.http_tools[0].method = "TRACE".into();
        assert!(validate_tool_config(&view).is_err());
    }
}

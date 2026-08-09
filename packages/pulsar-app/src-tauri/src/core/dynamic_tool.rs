//! 配置驱动 DynamicTool：由 `dynamic_tools.json` 声明、启动期装配的具名工具。
//!
//! - `HttpTool`：具名固定端点，模型只能填 `{param}` 占位参数值，不能改 URL。
//! - `CommandTool`：固定命令模板 + 参数占位；最终命令必须通过 `cmd_exec` 安全护栏
//!   （denylist / 超时夹紧 / 并发 / 输出截断 / 日志脱敏），这是「全量保留」的前提。
//!
//! 来源标记 `Config`，豁免 insert 门禁（声明即 schema）。

use crate::core::{
    cmd_exec,
    error::AppResult,
    tool_config::{CommandToolConfig, HttpToolConfig},
    tool_registry::Tool,
};
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;
use tokio::sync::Semaphore;

/// 配置驱动命令工具共享并发上限（与 execute_command 一致）。
const MAX_CONCURRENT: usize = cmd_exec::MAX_CONCURRENT;
/// HTTP 响应体最大字符数（截断阈值，复用 cmd_exec 的量级）。
const MAX_BODY_CHARS: usize = 64 * 1024;

/// 从模板中提取 `{param}` 占位符（去重、保序）。
fn extract_placeholders(template: &str) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        if let Some(rel_end) = rest[start..].find('}') {
            let key = &rest[start + 1..start + rel_end];
            if !key.is_empty() && !keys.iter().any(|k| k == key) {
                keys.push(key.to_string());
            }
            rest = &rest[start + rel_end + 1..];
        } else {
            break;
        }
    }
    keys
}

/// 由占位符构建 JSON Schema parameters。
fn build_params_schema(keys: &[String]) -> serde_json::Value {
    let properties = keys
        .iter()
        .map(|k| (k.clone(), json!({"type": "string", "description": k})))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "type": "object",
        "properties": properties,
        "required": keys,
    })
}

/// 用模型参数渲染模板；缺失占位符返回 None。
fn render_template(template: &str, keys: &[String], args: &serde_json::Value) -> Option<String> {
    let mut out = template.to_string();
    for key in keys {
        let value = args.get(key).and_then(|v| v.as_str())?;
        out = out.replace(&format!("{{{key}}}"), value);
    }
    // 校验无残留占位符（参数值内合法出现的字面花括号不受影响）。
    if keys.iter().any(|k| out.contains(&format!("{{{k}}}"))) {
        return None;
    }
    Some(out)
}

/// URL 参数值需要 percent-encode，避免参数值改变端点语义。
fn render_url(template: &str, keys: &[String], args: &serde_json::Value) -> Option<String> {
    let mut out = template.to_string();
    for key in keys {
        let raw = args.get(key).and_then(|v| v.as_str())?;
        out = out.replace(&format!("{{{key}}}"), &percent_encode(raw));
    }
    Some(out)
}

fn percent_encode(s: &str) -> String {
    // 仅编码会改变 URL 语义的字符；保留普通字母数字与 -_.~。
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 具名固定端点 HTTP 工具。
pub struct HttpTool {
    name: String,
    description: String,
    parameters: serde_json::Value,
    method: String,
    url: String,
    keys: Vec<String>,
    timeout: Duration,
}

impl HttpTool {
    pub fn from_config(cfg: &HttpToolConfig) -> Self {
        let keys = extract_placeholders(&cfg.url);
        let method = cfg.method.to_uppercase();
        Self {
            name: cfg.name.clone(),
            description: cfg.desc.clone(),
            parameters: build_params_schema(&keys),
            method,
            url: cfg.url.clone(),
            keys,
            timeout: Duration::from_millis(
                cfg.timeout_ms
                    .unwrap_or(cmd_exec::DEFAULT_TIMEOUT_MS)
                    .clamp(cmd_exec::MIN_TIMEOUT_MS, cmd_exec::MAX_TIMEOUT_MS),
            ),
        }
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.parameters.clone()
    }

    async fn execute(&self, args: serde_json::Value) -> AppResult<String> {
        let url = render_url(&self.url, &self.keys, &args).ok_or_else(|| {
            crate::core::error::AppError::InvalidInput(format!(
                "{}: 缺少必需参数，需要占位符 {:?}",
                self.name, self.keys
            ))
        })?;

        let client = reqwest::Client::new();
        let started = std::time::Instant::now();
        let response = tokio::time::timeout(self.timeout, {
            let method = self.method.clone();
            let url = url.clone();
            async move {
                let req = match method.as_str() {
                    "POST" => client.post(&url),
                    "PUT" => client.put(&url),
                    "DELETE" => client.delete(&url),
                    _ => client.get(&url),
                };
                req.send().await
            }
        })
        .await
        .map_err(|_| {
            crate::core::error::AppError::RuntimeError(format!(
                "{}: 请求超时（{}ms）",
                self.name,
                self.timeout.as_millis()
            ))
        })?
        .map_err(|e| {
            crate::core::error::AppError::RuntimeError(format!("{}: 请求失败: {e}", self.name))
        })?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| {
                crate::core::error::AppError::RuntimeError(format!(
                    "{}: 读取响应失败: {e}",
                    self.name
                ))
            })?
            .chars()
            .take(MAX_BODY_CHARS)
            .collect::<String>();

        tracing::info!(
            tool = self.name.as_str(),
            method = self.method.as_str(),
            url_len = url.len(),
            status = status.as_u16(),
            duration_ms = started.elapsed().as_millis() as u64,
            "config http tool executed"
        );

        if !status.is_success() {
            return Ok(json!({ "status": status.as_u16(), "body": body }).to_string());
        }
        Ok(body)
    }
}

/// 固定命令模板工具（复用 cmd_exec 安全护栏）。
pub struct CommandTool {
    name: String,
    description: String,
    parameters: serde_json::Value,
    template: String,
    keys: Vec<String>,
    timeout_ms: Option<u64>,
    semaphore: Semaphore,
}

impl CommandTool {
    pub fn from_config(cfg: &CommandToolConfig) -> Self {
        let keys = extract_placeholders(&cfg.template);
        Self {
            name: cfg.name.clone(),
            description: cfg.desc.clone(),
            parameters: build_params_schema(&keys),
            template: cfg.template.clone(),
            keys,
            timeout_ms: cfg.timeout_ms,
            semaphore: Semaphore::new(MAX_CONCURRENT),
        }
    }
}

#[async_trait]
impl Tool for CommandTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.parameters.clone()
    }

    async fn execute(&self, args: serde_json::Value) -> AppResult<String> {
        let command = render_template(&self.template, &self.keys, &args).ok_or_else(|| {
            crate::core::error::AppError::InvalidInput(format!(
                "{}: 缺少必需参数，需要占位符 {:?}",
                self.name, self.keys
            ))
        })?;

        if command.trim().is_empty() {
            return Err(crate::core::error::AppError::InvalidInput(format!(
                "{}: 渲染后的命令为空",
                self.name
            )));
        }
        // 硬约束：最终命令必须通过 denylist（模板本身在装配期已校验，此处兜底参数注入）。
        if cmd_exec::is_denied(&command) {
            return Err(crate::core::error::AppError::InvalidInput(format!(
                "{}: 命令被安全策略拒绝",
                self.name
            )));
        }

        let _permit = self.semaphore.acquire().await.map_err(|e| {
            crate::core::error::AppError::RuntimeError(format!(
                "{}: semaphore acquire failed: {e}",
                self.name
            ))
        })?;

        cmd_exec::run_guarded_shell(&self.name, &command, None, self.timeout_ms).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_placeholders_dedupes_and_keeps_order() {
        assert_eq!(
            extract_placeholders("a{query}b{query}c{page}"),
            vec!["query", "page"]
        );
        assert_eq!(extract_placeholders("no-placeholder"), Vec::<String>::new());
    }

    #[test]
    fn render_template_fills_and_rejects_missing() {
        let keys = vec!["query".to_string()];
        let args = json!({ "query": "hello" });
        assert_eq!(
            render_template("grep {query}", &keys, &args).as_deref(),
            Some("grep hello")
        );
        assert!(render_template("grep {query}", &keys, &json!({})).is_none());
    }

    #[test]
    fn percent_encode_keeps_safe_chars() {
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(percent_encode("你好"), "%E4%BD%A0%E5%A5%BD");
        assert_eq!(percent_encode("a-b_c.d~"), "a-b_c.d~");
    }

    #[tokio::test]
    async fn command_tool_denied_command_errors() {
        let cfg = CommandToolConfig {
            name: "bad".into(),
            desc: "bad".into(),
            template: "rm -rf /".into(),
            timeout_ms: None,
        };
        let tool = CommandTool::from_config(&cfg);
        assert!(tool.execute(json!({})).await.is_err());
    }

    #[tokio::test]
    async fn command_tool_runs_through_guarded_shell() {
        let cfg = CommandToolConfig {
            name: "greet".into(),
            desc: "greet".into(),
            template: "echo hello-{name}".into(),
            timeout_ms: Some(5000),
        };
        let tool = CommandTool::from_config(&cfg);
        let out = tool.execute(json!({ "name": "dyn" })).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["exit_code"], 0);
        assert!(v["stdout"].as_str().unwrap().contains("hello-dyn"));
    }
}

//! 内置工具：获取系统当前时间。
//!
//! 返回 Unix 毫秒时间戳、UTC ISO 8601 时间与本地时间（含时区偏移），
//! 供模型回答「现在几点 / 当前日期 / 时区」等时间相关问题。
//! 无参数、无副作用，不参与并发限制。

use async_trait::async_trait;
use chrono::{Local, SecondsFormat, Utc};
use serde_json::json;

use super::{error::AppResult, tool_registry::Tool};

/// 获取系统当前时间的 Agent 工具。
pub struct GetCurrentTimeTool;

impl GetCurrentTimeTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GetCurrentTimeTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GetCurrentTimeTool {
    fn name(&self) -> &str {
        "get_current_time"
    }

    fn description(&self) -> &str {
        "Get the current system time, returning the Unix millisecond timestamp, UTC ISO 8601 string, and local time with timezone offset."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _args: serde_json::Value) -> AppResult<String> {
        let now_utc = Utc::now();
        let now_local = Local::now();
        let result = json!({
            "unix_ms": now_utc.timestamp_millis(),
            "utc": now_utc.to_rfc3339_opts(SecondsFormat::Millis, true),
            "local": now_local.to_rfc3339_opts(SecondsFormat::Secs, false),
        });
        Ok(result.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_returns_all_time_fields() {
        let tool = GetCurrentTimeTool::new();
        let out = tool.execute(json!({})).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["unix_ms"].as_i64().unwrap() > 0, "unix_ms missing");
        let utc = v["utc"].as_str().expect("utc missing");
        assert!(utc.ends_with('Z'), "utc should be Z-suffixed, got {utc}");
        let local = v["local"].as_str().expect("local missing");
        assert!(
            !local.ends_with('Z'),
            "local should carry offset, got {local}"
        );
    }
}

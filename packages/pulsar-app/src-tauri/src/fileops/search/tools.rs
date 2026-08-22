//! `semantic_search` AI 原生工具：块级语义检索 active workspace。
//!
//! 与 `grep`/`glob` 同族（native + Core 标签，任何对话都带上），
//! 以 active workspace 为根，复用 `FileToolContext` 的越界护栏。
//! 返回「代码块」（函数/结构体/impl 等完整语义单元）而非命中行。

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::core::{
    error::{AppError, AppResult},
    tool_registry::Tool,
};
use crate::fileops::fs_tools::FileToolContext;
use crate::fileops::workspace::WorkspaceEntry;

use super::retriever::Retriever;

/// 语义搜索工具（手动实现 `Tool`，因 `file_tool!` 宏仅定义于 fs_tools 模块文本作用域）。
pub struct SemanticSearchTool {
    ctx: Arc<FileToolContext>,
}

impl SemanticSearchTool {
    pub fn new(ctx: Arc<FileToolContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl Tool for SemanticSearchTool {
    fn name(&self) -> &str {
        "semantic_search"
    }

    fn description(&self) -> &str {
        "Semantically search code blocks (functions, classes, structs, impls, traits) across the active workspace. Returns whole code units with line ranges rather than single matching lines. Prefer this over grep when you want to locate where a concept is implemented without knowing the exact identifiers."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query: natural language or keywords about the code you are looking for"
                },
                "top_k": {
                    "type": "integer",
                    "description": "Maximum number of blocks to return (default 10, max 20)"
                },
                "path": {
                    "type": "string",
                    "description": "Optional path prefix (relative to the workspace root) to limit the search to a single file or directory"
                }
            },
            "required": ["query"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let ws: WorkspaceEntry = self.ctx.active()?;
        let query = require_str(&args, "query")?;
        let top_k = opt_usize(&args, "top_k");
        let path = opt_str(&args, "path");
        let Some(index_root) = self.ctx.search_index_root() else {
            return Err(AppError::RuntimeError(
                "semantic search index root is not configured".into(),
            ));
        };
        let result = Retriever::search(&index_root, &ws, &query, top_k, path.as_deref())?;
        serde_json::to_string(&result)
            .map_err(|e| AppError::RuntimeError(format!("serialize search result: {e}")))
    }
}

fn require_str(args: &Value, key: &str) -> AppResult<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::InvalidInput(format!("missing string argument: {key}")))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn opt_usize(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(|v| v.as_u64()).map(|n| n as usize)
}

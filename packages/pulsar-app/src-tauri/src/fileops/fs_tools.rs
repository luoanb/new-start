//! AI 原生文件工具（native 通道，均有 `inserts/<name>.md` 门禁）。
//!
//! 工具与前端 UI 共用同一 `FileSystem` + `WorkspaceStore`：同一份越界护栏、
//! 同一份「已读清单」，保证 AI 与用户看到/操作同一文件状态。
//!
//! 工具语义对齐当前 IDE 文件工具：Read（offset/limit 分段读大文件）、
//! Write（覆盖已存在须先 Read）、SearchReplace（SEARCH/REPLACE 首处匹配）、
//! DeleteFile（一次多文件）、Glob、Grep（正则/大小写/多行/类型过滤/计数）、
//! LS、file_info、create_directory、rename/move。

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::core::{
    error::{AppError, AppResult},
    tool_registry::{Tool, ToolRegistry},
};
use super::{
    fs::FileSystem,
    workspace::{WorkspaceEntry, WorkspaceStore},
};

/// 文件工具共享上下文：工作区存储 + 文件操作层。
pub struct FileToolContext {
    store: Arc<WorkspaceStore>,
    fs: Arc<FileSystem>,
}

impl FileToolContext {
    pub fn new(store: Arc<WorkspaceStore>, fs: Arc<FileSystem>) -> Self {
        Self { store, fs }
    }

    /// 取当前 active 工作区（工具统一以 active workspace 为根）。
    fn active(&self) -> AppResult<WorkspaceEntry> {
        self.store
            .active()?
            .ok_or_else(|| AppError::InvalidInput(
                "no active workspace; add one via the Files view before using file tools".into(),
            ))
    }
}

/// 把全部文件工具注册进 registry（native + Core 标签，任何对话都带上）。
pub fn register_file_tools(registry: &mut ToolRegistry, ctx: Arc<FileToolContext>) {
    registry.register_core(ListDirectoryTool::new(Arc::clone(&ctx)));
    registry.register_core(ReadFileTool::new(Arc::clone(&ctx)));
    registry.register_core(WriteFileTool::new(Arc::clone(&ctx)));
    registry.register_core(SearchReplaceTool::new(Arc::clone(&ctx)));
    registry.register_core(DeleteFileTool::new(Arc::clone(&ctx)));
    registry.register_core(GlobTool::new(Arc::clone(&ctx)));
    registry.register_core(GrepTool::new(Arc::clone(&ctx)));
    registry.register_core(FileInfoTool::new(Arc::clone(&ctx)));
    registry.register_core(CreateDirectoryTool::new(Arc::clone(&ctx)));
    registry.register_core(RenameTool::new(Arc::clone(&ctx)));
}

/// 工具参数通用提取：缺失/类型错误 → 可读 InvalidInput。
fn require_str(args: &Value, key: &str) -> AppResult<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::InvalidInput(format!("missing string argument: {key}")))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn opt_usize(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(|v| v.as_u64()).map(|n| n as usize)
}

fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(|v| v.as_bool())
}

fn opt_str_vec(args: &Value, key: &str) -> Option<Vec<String>> {
    args.get(key).and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    })
}

/// 工具结果统一序列化为 JSON 字符串。
fn ok_json<T: serde::Serialize>(value: T) -> AppResult<String> {
    serde_json::to_string(&value)
        .map_err(|e| AppError::RuntimeError(format!("serialize tool result: {e}")))
}

/// 声明一个文件工具：样板（struct + Tool impl）由宏生成，业务闭包只负责参数→结果。
macro_rules! file_tool {
    ($ty:ident, $id:literal, $desc:literal, $params:tt, $exec:expr) => {
        pub struct $ty {
            ctx: Arc<FileToolContext>,
        }
        impl $ty {
            pub fn new(ctx: Arc<FileToolContext>) -> Self {
                Self { ctx }
            }
        }
        #[async_trait]
        impl Tool for $ty {
            fn name(&self) -> &str {
                $id
            }
            fn description(&self) -> &str {
                $desc
            }
            fn parameters(&self) -> Value {
                json!($params)
            }
            async fn execute(&self, args: Value) -> AppResult<String> {
                let ws = self.ctx.active()?;
                let out = ($exec)(&self.ctx, &ws, &args)?;
                ok_json(out)
            }
        }
    };
}

file_tool!(
    ListDirectoryTool,
    "list_directory",
    "List the entries (files and directories) inside a directory of the active workspace, filtered by the workspace ignore rules. Mirrors the LS capability of an IDE. Paths are relative to the active workspace root; the root itself is used when path is omitted. Results are sorted with directories first.",
    {
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "Directory relative to the active workspace root; omit to list the workspace root"},
            "ignore": {"type": "array", "items": {"type": "string"}, "description": "Optional extra ignore rules (glob/prefix) applied on top of the workspace defaults"}
        },
        "additionalProperties": false
    },
    |ctx: &FileToolContext, ws: &WorkspaceEntry, args: &Value| -> AppResult<Value> {
        let ignore = opt_str_vec(args, "ignore").map(|v| v);
        let entries = ctx.fs.list(ws, opt_str(args, "path").as_deref(), ignore.as_deref())?;
        Ok(serde_json::to_value(entries).map_err(to_runtime)?)
    }
);

file_tool!(
    ReadFileTool,
    "read_file",
    "Read a text file from the active workspace with optional line-based paging (offset = starting line, 0-based; limit = max lines). Mirrors the Read capability of an IDE. Reading a file marks it as read, which is required before it can be overwritten. Binary files are rejected.",
    {
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "File path relative to the active workspace root"},
            "offset": {"type": "integer", "description": "Starting line (0-based); omit to read from the beginning"},
            "limit": {"type": "integer", "description": "Maximum number of lines to return; omit to read until the default chunk size"}
        },
        "required": ["path"],
        "additionalProperties": false
    },
    |ctx: &FileToolContext, ws: &WorkspaceEntry, args: &Value| -> AppResult<Value> {
        let path = require_str(args, "path")?;
        let result = ctx.fs.read(ws, &path, opt_usize(args, "offset"), opt_usize(args, "limit"))?;
        Ok(serde_json::to_value(result).map_err(to_runtime)?)
    }
);

file_tool!(
    WriteFileTool,
    "write_file",
    "Write content to a file inside the active workspace. New files can be written freely; overwriting an existing file requires that the file was previously read (read_file) and not modified on disk since. Mirrors the Write capability of an IDE.",
    {
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "File path relative to the active workspace root"},
            "content": {"type": "string", "description": "Full new content of the file"}
        },
        "required": ["path", "content"],
        "additionalProperties": false
    },
    |ctx: &FileToolContext, ws: &WorkspaceEntry, args: &Value| -> AppResult<Value> {
        let path = require_str(args, "path")?;
        let content = require_str(args, "content")?;
        let result = ctx.fs.write(ws, &path, &content, None)?;
        Ok(serde_json::to_value(result).map_err(to_runtime)?)
    }
);

file_tool!(
    SearchReplaceTool,
    "search_replace",
    "Replace the first occurrence of a literal search string in a text file with the replacement text. Mirrors the SearchReplace capability of an IDE. The file must have been previously read and not modified on disk since. Returns matched=true when a replacement happened.",
    {
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "File path relative to the active workspace root"},
            "search": {"type": "string", "description": "Literal text to find (first occurrence only)"},
            "replace": {"type": "string", "description": "Replacement text"}
        },
        "required": ["path", "search", "replace"],
        "additionalProperties": false
    },
    |ctx: &FileToolContext, ws: &WorkspaceEntry, args: &Value| -> AppResult<Value> {
        let path = require_str(args, "path")?;
        let search = require_str(args, "search")?;
        let replace = require_str(args, "replace")?;
        let matched = ctx.fs.search_replace(ws, &path, &search, &replace)?;
        Ok(json!({ "matched": matched }))
    }
);

file_tool!(
    DeleteFileTool,
    "delete_file",
    "Delete one or more files or directories (recursively) inside the active workspace. Mirrors the DeleteFile capability of an IDE. All paths must exist and stay within the workspace root. There is no undo.",
    {
        "type": "object",
        "properties": {
            "paths": {"type": "array", "items": {"type": "string"}, "description": "Paths relative to the active workspace root to delete"}
        },
        "required": ["paths"],
        "additionalProperties": false
    },
    |ctx: &FileToolContext, ws: &WorkspaceEntry, args: &Value| -> AppResult<Value> {
        let paths = opt_str_vec(args, "paths").unwrap_or_default();
        ctx.fs.delete(ws, &paths)?;
        Ok(Value::Null)
    }
);

file_tool!(
    GlobTool,
    "glob",
    "Find files matching a glob pattern inside the active workspace (recursive), sorted by modification time. Mirrors the Glob capability of an IDE. Results are limited; use a narrower pattern when possible.",
    {
        "type": "object",
        "properties": {
            "pattern": {"type": "string", "description": "Glob pattern relative to the workspace root, e.g. '**/*.rs'"},
            "cwd": {"type": "string", "description": "Optional directory to start the search from, relative to the workspace root"}
        },
        "required": ["pattern"],
        "additionalProperties": false
    },
    |ctx: &FileToolContext, ws: &WorkspaceEntry, args: &Value| -> AppResult<Value> {
        let pattern = require_str(args, "pattern")?;
        let results = ctx.fs.glob(ws, &pattern, opt_str(args, "cwd").as_deref())?;
        Ok(serde_json::to_value(results).map_err(to_runtime)?)
    }
);

file_tool!(
    GrepTool,
    "grep",
    "Search file contents with a regular expression inside the active workspace. Mirrors the Grep capability of an IDE: supports case sensitivity, multiline (^ $ anchors), a glob type filter, and context lines. The count of matches is the length of the returned array.",
    {
        "type": "object",
        "properties": {
            "pattern": {"type": "string", "description": "Regular expression to search for"},
            "path": {"type": "string", "description": "Optional directory to limit the search to, relative to the workspace root"},
            "case_sensitive": {"type": "boolean", "description": "Whether matching is case sensitive (default false)"},
            "multiline": {"type": "boolean", "description": "Whether ^ and $ match at line boundaries (default false)"},
            "glob": {"type": "string", "description": "Optional glob type filter on file paths, e.g. '*.rs'"},
            "context": {"type": "integer", "description": "Number of context lines before/after each match (default 0)"}
        },
        "required": ["pattern"],
        "additionalProperties": false
    },
    |ctx: &FileToolContext, ws: &WorkspaceEntry, args: &Value| -> AppResult<Value> {
        let pattern = require_str(args, "pattern")?;
        let context = opt_usize(args, "context").unwrap_or(0);
        let matches = ctx.fs.grep(
            ws,
            &pattern,
            opt_str(args, "path").as_deref(),
            opt_bool(args, "case_sensitive").unwrap_or(false),
            opt_bool(args, "multiline").unwrap_or(false),
            opt_str(args, "glob").as_deref(),
            context,
        )?;
        Ok(serde_json::to_value(matches).map_err(to_runtime)?)
    }
);

file_tool!(
    FileInfoTool,
    "file_info",
    "Return metadata for a file or directory inside the active workspace: existence, type, size, modified time, and whether the file is binary.",
    {
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "Path relative to the active workspace root"}
        },
        "required": ["path"],
        "additionalProperties": false
    },
    |ctx: &FileToolContext, ws: &WorkspaceEntry, args: &Value| -> AppResult<Value> {
        let path = require_str(args, "path")?;
        let info = ctx.fs.info(ws, &path)?;
        Ok(serde_json::to_value(info).map_err(to_runtime)?)
    }
);

file_tool!(
    CreateDirectoryTool,
    "create_directory",
    "Create a directory (and any missing parents) inside the active workspace. Fails if a file exists at the target path.",
    {
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "Directory path relative to the active workspace root"}
        },
        "required": ["path"],
        "additionalProperties": false
    },
    |ctx: &FileToolContext, ws: &WorkspaceEntry, args: &Value| -> AppResult<Value> {
        let path = require_str(args, "path")?;
        ctx.fs.create_dir(ws, &path)?;
        Ok(Value::Null)
    }
);

file_tool!(
    RenameTool,
    "rename",
    "Rename or move a file or directory inside the active workspace. from and to are both relative to the workspace root; use the same parent directory to rename in place, a different parent to move. The target must not already exist.",
    {
        "type": "object",
        "properties": {
            "from": {"type": "string", "description": "Source path relative to the active workspace root"},
            "to": {"type": "string", "description": "Destination path relative to the active workspace root"}
        },
        "required": ["from", "to"],
        "additionalProperties": false
    },
    |ctx: &FileToolContext, ws: &WorkspaceEntry, args: &Value| -> AppResult<Value> {
        let from = require_str(args, "from")?;
        let to = require_str(args, "to")?;
        ctx.fs.rename(ws, &from, &to)?;
        Ok(Value::Null)
    }
);

fn to_runtime(e: impl std::fmt::Display) -> AppError {
    AppError::RuntimeError(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fileops::workspace::WorkspaceStore;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn setup(name: &str) -> (PathBufForTest, Arc<FileToolContext>) {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let root = std::env::temp_dir().join(format!("pulsar-{name}-{ms}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let ws = root.join("proj");
        fs::create_dir_all(&ws).unwrap();
        fs::write(ws.join("a.txt"), "hello\nworld\n").unwrap();
        let store = Arc::new(WorkspaceStore::new(&root).unwrap());
        store.add(ws.to_str().unwrap()).unwrap();
        let ctx = Arc::new(FileToolContext::new(Arc::clone(&store), Arc::new(FileSystem::new())));
        (PathBufForTest(root), ctx)
    }

    struct PathBufForTest(std::path::PathBuf);
    impl PathBufForTest {
        fn cleanup(&self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    #[tokio::test]
    async fn list_directory_tool() {
        let (root, ctx) = setup("tool_list");
        let tool = ListDirectoryTool::new(ctx);
        let out = tool.execute(json!({})).await.unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let entries = v.as_array().unwrap();
        assert!(entries.iter().any(|e| e["name"] == "a.txt"));
        root.cleanup();
    }

    #[tokio::test]
    async fn read_write_search_replace_flow() {
        let (root, ctx) = setup("tool_rw");
        let read_tool = ReadFileTool::new(Arc::clone(&ctx));
        let write_tool = WriteFileTool::new(Arc::clone(&ctx));
        let sr_tool = SearchReplaceTool::new(Arc::clone(&ctx));

        // 未读直接写已存在 → 拒绝。
        let err = write_tool
            .execute(json!({"path": "a.txt", "content": "x"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("must be read before overwriting"));

        // 读后 search_replace。
        read_tool.execute(json!({"path": "a.txt"})).await.unwrap();
        let out = sr_tool
            .execute(json!({"path": "a.txt", "search": "world", "replace": "there"}))
            .await
            .unwrap();
        assert!(out.contains("\"matched\":true"));
        let out = read_tool.execute(json!({"path": "a.txt"})).await.unwrap();
        assert!(out.contains("there"));

        // 读后 write。
        write_tool
            .execute(json!({"path": "a.txt", "content": "new"}))
            .await
            .unwrap();
        let out = read_tool.execute(json!({"path": "a.txt"})).await.unwrap();
        assert!(out.contains("new"));
        root.cleanup();
    }

    #[tokio::test]
    async fn glob_and_grep_tools() {
        let (root, ctx) = setup("tool_glob");
        fs::write(root.0.join("proj/b.rs"), "fn main() {}\n").unwrap();
        let glob_tool = GlobTool::new(Arc::clone(&ctx));
        let out = glob_tool.execute(json!({"pattern": "**/*.rs"})).await.unwrap();
        assert!(out.contains("b.rs"));
        let grep_tool = GrepTool::new(Arc::clone(&ctx));
        let out = grep_tool
            .execute(json!({"pattern": "hello", "glob": "*.txt"}))
            .await
            .unwrap();
        assert!(out.contains("\"text\":\"hello\""));
        root.cleanup();
    }
}

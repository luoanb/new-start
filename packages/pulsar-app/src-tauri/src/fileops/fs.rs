//! 工作区文件操作层：list / read / write / create_dir / delete / rename / move /
//! glob / grep / info。
//!
//! 安全护栏（与 AI 工具共用同一实现）：
//! - 所有路径经 `WorkspaceStore::resolve_in_workspace` 越界校验；
//! - 二进制检测（NUL 字节启发式）；
//! - 读大小防御上限 + 行分段（offset/limit）；
//! - 写大小上限；
//! - 覆盖已存在文件前须先 Read（进程内存「已读清单」，按路径 → mtime 校验）；
//! - 保存前外部修改检测（磁盘 mtime != 已读时 mtime → 拒绝）。

use crate::core::error::{AppError, AppResult};
use super::workspace::WorkspaceStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// 读防御上限：单次读取超过该字节数拒绝（避免 AI 拉爆内存）。
pub const MAX_READ_BYTES: u64 = 64 * 1024 * 1024;
/// 未指定 offset/limit 时默认返回的内容阈值（按字节截断行）。
pub const DEFAULT_READ_CHUNK_BYTES: u64 = 256 * 1024;
/// 写大小上限。
pub const MAX_WRITE_BYTES: u64 = 16 * 1024 * 1024;
/// grep 结果上限（防爆量）。
pub const MAX_GREP_MATCHES: usize = 2000;
/// glob 结果上限。
pub const MAX_GLOB_RESULTS: usize = 1000;
/// 二进制检测探测字节数。
const BINARY_PROBE_BYTES: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsEntry {
    pub name: String,
    /// 相对 workspace 根（`/` 分隔，无前导斜杠）。
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsReadResult {
    pub content: String,
    pub total_lines: usize,
    pub total_chars: usize,
    pub mtime_ms: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsWriteResult {
    pub mtime_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsMatch {
    pub path: String,
    pub modified_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    pub path: String,
    /// 1-based 行号。
    pub line: usize,
    /// 行内列偏移（字节，0-based）。
    pub column: usize,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_before: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_after: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsInfo {
    pub exists: bool,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub is_binary: bool,
}

/// 已读标记：写前校验用（路径 → 读取时的 mtime）。
#[derive(Debug, Clone, Copy)]
struct ReadMark {
    mtime_ms: i64,
}

/// 文件操作层。所有写操作前校验：
/// 1. 越界（resolve_in_workspace）；2. 大小上限；3. 覆盖已存在须已读且未外部修改。
#[derive(Debug, Default)]
pub struct FileSystem {
    /// canonicalized 绝对路径 → 读取时 mtime。
    read_marks: RwLock<HashMap<PathBuf, ReadMark>>,
}

fn to_rel(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root)
        .map(|p| p.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
        .unwrap_or_else(|_| abs.display().to_string())
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_PROBE_BYTES)].contains(&0)
}

fn mtime_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

/// ignore 规则匹配：前缀/glob 语义。
/// - 含 glob 通配符（`* ? [`）→ 按 glob 匹配相对路径；
/// - 否则匹配任意层级的该名称条目（如 `node_modules` 匹配任何层级的 node_modules）。
pub fn is_ignored(rel: &str, ignore: &[String]) -> bool {
    let rel_norm = rel.trim_start_matches('/');
    ignore.iter().any(|pat| {
        let pat = pat.trim();
        if pat.is_empty() {
            return false;
        }
        if pat.contains('*') || pat.contains('?') || pat.contains('[') {
            if let Ok(glob) = globset::Glob::new(pat) {
                let matcher = glob.compile_matcher();
                return matcher.is_match(rel_norm) || matcher.is_match(&format!("/{rel_norm}"));
            }
            false
        } else {
            rel_norm == pat
                || rel_norm.starts_with(&format!("{pat}/"))
                || rel_norm.split('/').any(|seg| seg == pat)
        }
    })
}

impl FileSystem {
    pub fn new() -> Self {
        Self {
            read_marks: RwLock::new(HashMap::new()),
        }
    }

    /// 列出目录内容（非递归），按工作区 ignore 过滤。
    /// `path` 空/缺省 = 工作区根。
    pub fn list(
        &self,
        workspace: &super::workspace::WorkspaceEntry,
        path: Option<&str>,
        ignore: Option<&[String]>,
    ) -> AppResult<Vec<FsEntry>> {
        let root = workspace.root.as_path();
        let dir = match path {
            Some(p) if !p.trim().is_empty() => WorkspaceStore::resolve_in_workspace(root, p)?,
            _ => root.to_path_buf(),
        };
        if !dir.is_dir() {
            return Err(AppError::InvalidInput(format!(
                "not a directory: {}",
                dir.display()
            )));
        }
        let ignore_rules = ignore.unwrap_or(&workspace.ignore);
        let mut entries = Vec::new();
        let rd = fs::read_dir(&dir)?;
        for item in rd {
            let item = item?;
            let name = item.file_name().to_string_lossy().into_owned();
            let file_type = item.file_type()?;
            // 符号链接一律不展示（避免逃逸与循环）。
            if file_type.is_symlink() {
                continue;
            }
            let abs = item.path();
            let rel = to_rel(root, &abs);
            if is_ignored(&rel, ignore_rules) {
                continue;
            }
            let is_dir = file_type.is_dir();
            let meta = item.metadata()?;
            entries.push(FsEntry {
                name,
                path: rel,
                is_dir,
                size: if is_dir { None } else { Some(meta.len()) },
                modified_ms: Some(mtime_ms(&meta)),
            });
        }
        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });
        Ok(entries)
    }

    /// 读文件：行分段（offset 起始行 0-based，limit 最大行数）。
    /// 记录「已读」标记（路径 → mtime）供覆盖校验。
    pub fn read(
        &self,
        workspace: &super::workspace::WorkspaceEntry,
        path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> AppResult<FsReadResult> {
        let abs = WorkspaceStore::resolve_in_workspace(&workspace.root, path)?;
        let meta = fs::metadata(&abs)?;
        if !meta.is_file() {
            return Err(AppError::InvalidInput(format!("not a file: {abs:?}")));
        }
        if meta.len() > MAX_READ_BYTES {
            return Err(AppError::InvalidInput(format!(
                "file too large to read ({} bytes > {} limit)",
                meta.len(),
                MAX_READ_BYTES
            )));
        }
        let bytes = fs::read(&abs)?;
        if is_binary(&bytes) {
            return Err(AppError::InvalidInput(format!(
                "binary file cannot be read as text: {path}"
            )));
        }
        let mtime = mtime_ms(&meta);
        // 记录已读标记。
        if let Ok(mut guard) = self.read_marks.write() {
            guard.insert(abs, ReadMark { mtime_ms: mtime });
        }

        let text = String::from_utf8_lossy(&bytes);
        let total_chars = text.chars().count();
        let lines: Vec<&str> = text.lines().collect();
        let total_lines = lines.len();
        let start = offset.unwrap_or(0).min(total_lines);
        // 未指定 offset/limit 时按 256KB 阈值截断；否则按行精确切片。
        let mut truncated = false;
        let end = match limit {
            Some(limit) if limit > 0 => {
                let end = (start + limit).min(total_lines);
                truncated = end < total_lines;
                end
            }
            _ => {
                let mut char_count = 0usize;
                let mut end = total_lines;
                for (idx, line) in lines.iter().enumerate().skip(start) {
                    char_count += line.chars().count() + 1; // +1 换行
                    if char_count as u64 > DEFAULT_READ_CHUNK_BYTES {
                        end = idx;
                        truncated = true;
                        break;
                    }
                }
                end
            }
        };
        let selected = &lines[start..end];
        let content = if selected.is_empty() {
            String::new()
        } else {
            format!("{}\n", selected.join("\n"))
        };
        Ok(FsReadResult {
            content,
            total_lines,
            total_chars,
            mtime_ms: mtime,
            truncated,
        })
    }

    /// 写文件（覆盖须先已读 + 外部修改检测）。
    /// `base_mtime` 由前端保存时携带（打开时 mtime）；AI 通道不传，走已读清单。
    pub fn write(
        &self,
        workspace: &super::workspace::WorkspaceEntry,
        path: &str,
        content: &str,
        base_mtime: Option<i64>,
    ) -> AppResult<FsWriteResult> {
        if content.len() as u64 > MAX_WRITE_BYTES {
            return Err(AppError::InvalidInput(format!(
                "content too large ({} bytes > {} limit)",
                content.len(),
                MAX_WRITE_BYTES
            )));
        }
        let abs = WorkspaceStore::resolve_in_workspace(&workspace.root, path)?;
        let existing = fs::metadata(&abs).ok();
        if let Some(meta) = existing {
            if !meta.is_file() {
                return Err(AppError::InvalidInput(format!("not a file: {abs:?}")));
            }
            let disk_mtime = mtime_ms(&meta);
            // 覆盖校验：前端传 base_mtime（打开时快照）；AI 走已读清单。
            let expected = match base_mtime {
                Some(base) => Some(base),
                None => self
                    .read_marks
                    .read()
                    .ok()
                    .and_then(|g| g.get(&abs).map(|m| m.mtime_ms)),
            };
            let Some(expected) = expected else {
                return Err(AppError::InvalidInput(format!(
                    "file must be read before overwriting: {path}"
                )));
            };
            if disk_mtime != expected {
                return Err(AppError::InvalidInput(format!(
                    "file was modified on disk since it was read; re-read before overwriting: {path}"
                )));
            }
        }
        fs::write(&abs, content)?;
        let new_mtime = fs::metadata(&abs).map(|m| mtime_ms(&m)).unwrap_or(0);
        // 更新已读标记为最新（本次写入即最新快照）。
        if let Ok(mut guard) = self.read_marks.write() {
            guard.insert(abs, ReadMark { mtime_ms: new_mtime });
        }
        Ok(FsWriteResult { mtime_ms: new_mtime })
    }

    /// 递归创建目录。
    pub fn create_dir(
        &self,
        workspace: &super::workspace::WorkspaceEntry,
        path: &str,
    ) -> AppResult<()> {
        let abs = WorkspaceStore::resolve_in_workspace(&workspace.root, path)?;
        fs::create_dir_all(&abs)?;
        Ok(())
    }

    /// 删除文件/目录（递归）。
    pub fn delete(
        &self,
        workspace: &super::workspace::WorkspaceEntry,
        paths: &[String],
    ) -> AppResult<()> {
        if paths.is_empty() {
            return Err(AppError::InvalidInput("no paths to delete".into()));
        }
        if paths.len() > 100 {
            return Err(AppError::InvalidInput("too many paths to delete".into()));
        }
        for rel in paths {
            let abs = WorkspaceStore::resolve_in_workspace(&workspace.root, rel)?;
            if !abs.exists() {
                return Err(AppError::InvalidInput(format!(
                    "path does not exist: {rel}"
                )));
            }
            let meta = fs::metadata(&abs)?;
            if meta.is_dir() {
                fs::remove_dir_all(&abs)?;
            } else {
                fs::remove_file(&abs)?;
            }
            // 删除后清除已读标记。
            if let Ok(mut guard) = self.read_marks.write() {
                guard.remove(&abs);
            }
        }
        Ok(())
    }

    /// SEARCH/REPLACE：在文件中替换首次出现的 `search` 字面量。
    /// 要求：文件已存在、已读（已读标记）且无外部修改；成功则更新已读标记。
    /// 返回 `matched`（是否找到并替换）。
    pub fn search_replace(
        &self,
        workspace: &super::workspace::WorkspaceEntry,
        path: &str,
        search: &str,
        replace: &str,
    ) -> AppResult<bool> {
        let abs = WorkspaceStore::resolve_in_workspace(&workspace.root, path)?;
        let meta = fs::metadata(&abs)?;
        if !meta.is_file() {
            return Err(AppError::InvalidInput(format!("not a file: {abs:?}")));
        }
        if meta.len() > MAX_READ_BYTES {
            return Err(AppError::InvalidInput(format!(
                "file too large to search_replace ({} bytes > {} limit)",
                meta.len(),
                MAX_READ_BYTES
            )));
        }
        let bytes = fs::read(&abs)?;
        if is_binary(&bytes) {
            return Err(AppError::InvalidInput(format!(
                "binary file cannot be edited as text: {path}"
            )));
        }
        let disk_mtime = mtime_ms(&meta);
        let expected = self
            .read_marks
            .read()
            .ok()
            .and_then(|g| g.get(&abs).map(|m| m.mtime_ms));
        let Some(expected) = expected else {
            return Err(AppError::InvalidInput(format!(
                "file must be read before editing: {path}"
            )));
        };
        if disk_mtime != expected {
            return Err(AppError::InvalidInput(format!(
                "file was modified on disk since it was read; re-read before editing: {path}"
            )));
        }
        let text = String::from_utf8_lossy(&bytes);
        let Some(pos) = text.find(search) else {
            return Ok(false);
        };
        let new_text = format!("{}{}{}", &text[..pos], replace, &text[pos + search.len()..]);
        fs::write(&abs, new_text)?;
        let new_mtime = fs::metadata(&abs).map(|m| mtime_ms(&m)).unwrap_or(0);
        if let Ok(mut guard) = self.read_marks.write() {
            guard.insert(abs, ReadMark { mtime_ms: new_mtime });
        }
        Ok(true)
    }

    /// 重命名 / 移动（同一底层 rename；越界由 resolve 双端保证）。
    pub fn rename(
        &self,
        workspace: &super::workspace::WorkspaceEntry,
        from: &str,
        to: &str,
    ) -> AppResult<()> {
        let src = WorkspaceStore::resolve_in_workspace(&workspace.root, from)?;
        let dst = WorkspaceStore::resolve_in_workspace(&workspace.root, to)?;
        if !src.exists() {
            return Err(AppError::InvalidInput(format!(
                "path does not exist: {from}"
            )));
        }
        if dst.exists() {
            return Err(AppError::InvalidInput(format!(
                "target already exists: {to}"
            )));
        }
        // 目标父目录须存在（不允许 rename 跨不存在的父级）。
        if let Some(parent) = dst.parent() {
            if !parent.exists() {
                return Err(AppError::InvalidInput(format!(
                    "target parent directory does not exist: {}",
                    parent.display()
                )));
            }
        }
        fs::rename(&src, &dst)?;
        if let Ok(mut guard) = self.read_marks.write() {
            if let Some(mark) = guard.remove(&src) {
                guard.insert(dst, mark);
            }
        }
        Ok(())
    }

    /// glob 查找：递归遍历 workspace（可选 cwd 限定子目录），按修改时间排序，结果上限。
    pub fn glob(
        &self,
        workspace: &super::workspace::WorkspaceEntry,
        pattern: &str,
        cwd: Option<&str>,
    ) -> AppResult<Vec<FsMatch>> {
        let root = workspace.root.as_path();
        let base = match cwd {
            Some(p) if !p.trim().is_empty() => {
                WorkspaceStore::resolve_in_workspace(root, p)?
            }
            _ => root.to_path_buf(),
        };
        let glob = globset::Glob::new(pattern)
            .map_err(|e| AppError::InvalidInput(format!("invalid glob pattern: {e}")))?
            .compile_matcher();
        let mut results = Vec::new();
        for entry in walkdir::WalkDir::new(&base)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let abs = entry.path();
            if abs == base {
                continue;
            }
            let rel = to_rel(root, abs);
            if glob.is_match(&rel) {
                let meta = match fs::metadata(abs) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                results.push(FsMatch {
                    path: rel,
                    modified_ms: mtime_ms(&meta),
                });
                if results.len() >= MAX_GLOB_RESULTS {
                    break;
                }
            }
        }
        results.sort_by_key(|m| m.modified_ms);
        Ok(results)
    }

    /// grep 内容搜索：正则 / 大小写 / 多行（`^$` 语义）/ glob 类型过滤 / context。
    pub fn grep(
        &self,
        workspace: &super::workspace::WorkspaceEntry,
        pattern: &str,
        path: Option<&str>,
        case_sensitive: bool,
        multiline: bool,
        glob: Option<&str>,
        context: usize,
    ) -> AppResult<Vec<GrepMatch>> {
        let root = workspace.root.as_path();
        let base = match path {
            Some(p) if !p.trim().is_empty() => WorkspaceStore::resolve_in_workspace(root, p)?,
            _ => root.to_path_buf(),
        };
        let regex = regex::RegexBuilder::new(pattern)
            .case_insensitive(!case_sensitive)
            .multi_line(multiline)
            .build()
            .map_err(|e| AppError::InvalidInput(format!("invalid regex pattern: {e}")))?;
        let type_filter = glob.map(|g| {
            globset::Glob::new(g)
                .map(|g| g.compile_matcher())
                .ok()
        }).flatten();

        let mut matches = Vec::new();
        for entry in walkdir::WalkDir::new(&base)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if matches.len() >= MAX_GREP_MATCHES {
                break;
            }
            let abs = entry.path();
            if !entry.file_type().is_file() || abs == base {
                continue;
            }
            let rel = to_rel(root, abs);
            if let Some(matcher) = &type_filter {
                if !matcher.is_match(&rel) {
                    continue;
                }
            }
            // 跳过明显二进制（快速探测），grep 文本语义。
            let bytes = match fs::read(abs) {
                Ok(b) => b,
                Err(_) => continue,
            };
            if is_binary(&bytes) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.split('\n').collect();
            for (idx, line) in lines.iter().enumerate() {
                if matches.len() >= MAX_GREP_MATCHES {
                    break;
                }
                if let Some(captured) = regex.find(line) {
                    let before = if context > 0 {
                        lines[..idx]
                            .iter()
                            .rev()
                            .take(context)
                            .rev()
                            .map(|l| l.to_string())
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let after = if context > 0 {
                        lines[idx + 1..]
                            .iter()
                            .take(context)
                            .map(|l| l.to_string())
                            .collect()
                    } else {
                        Vec::new()
                    };
                    matches.push(GrepMatch {
                        path: rel.clone(),
                        line: idx + 1,
                        column: captured.start(),
                        text: line.to_string(),
                        context_before: before,
                        context_after: after,
                    });
                }
            }
        }
        Ok(matches)
    }

    /// 元信息。
    pub fn info(
        &self,
        workspace: &super::workspace::WorkspaceEntry,
        path: &str,
    ) -> AppResult<FsInfo> {
        let abs = WorkspaceStore::resolve_in_workspace(&workspace.root, path)?;
        let Ok(meta) = fs::metadata(&abs) else {
            return Ok(FsInfo {
                exists: false,
                is_dir: false,
                size: 0,
                modified_ms: None,
                is_binary: false,
            });
        };
        let is_dir = meta.is_dir();
        let size = if is_dir { 0 } else { meta.len() };
        let is_binary = if is_dir {
            false
        } else {
            fs::read(&abs).map(|b| is_binary(&b)).unwrap_or(false)
        };
        Ok(FsInfo {
            exists: true,
            is_dir,
            size,
            modified_ms: Some(mtime_ms(&meta)),
            is_binary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fileops::workspace::{WorkspaceEntry, WorkspaceStore};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::env::temp_dir().join(format!("pulsar-{name}-{ms}"))
    }

    /// 创建临时工作区：root/proj，返回 (root, WorkspaceEntry)。
    fn setup(name: &str) -> (PathBuf, WorkspaceEntry) {
        let root = test_root(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let ws = root.join("proj");
        fs::create_dir_all(&ws).unwrap();
        fs::write(ws.join("a.txt"), "hello\nworld\n").unwrap();
        fs::write(ws.join("b.rs"), "fn main() {}\n").unwrap();
        fs::create_dir_all(ws.join("src/lib")).unwrap();
        fs::write(ws.join("src/lib/mod.rs"), "pub mod x;\n").unwrap();
        fs::create_dir_all(ws.join("node_modules/pkg")).unwrap();
        fs::write(ws.join("node_modules/pkg/index.js"), "x\n").unwrap();
        let store = WorkspaceStore::new(&root).unwrap();
        let view = store.add(ws.to_str().unwrap()).unwrap();
        (root, view.workspaces[0].clone())
    }

    #[test]
    fn list_root_with_ignore_filter() {
        let (root, ws) = setup("list_root");
        let fs = FileSystem::new();
        let entries = fs.list(&ws, None, None).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // node_modules 被默认 ignore 过滤；src 目录排在文件前。
        assert!(names.contains(&"src"));
        assert!(names.contains(&"a.txt"));
        assert!(!names.contains(&"node_modules"));
        assert!(entries[0].is_dir, "directories first");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn read_and_write_roundtrip_with_read_guard() {
        let (root, ws) = setup("read_write");
        let fs = FileSystem::new();
        let r = fs.read(&ws, "a.txt", None, None).unwrap();
        assert_eq!(r.content, "hello\nworld\n");
        assert_eq!(r.total_lines, 2);
        assert!(!r.truncated);
        // 未读先写 → 拒绝。
        let err = fs.write(&ws, "b.rs", "x", None).unwrap_err();
        assert!(err.to_string().contains("must be read before overwriting"));
        // 已读后写 → 成功。
        fs.read(&ws, "b.rs", None, None).unwrap();
        fs.write(&ws, "b.rs", "fn main() { println!(); }\n", None).unwrap();
        let r2 = fs.read(&ws, "b.rs", None, None).unwrap();
        assert!(r2.content.contains("println!"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn read_offset_limit_slices_lines() {
        let (root, ws) = setup("read_slice");
        fs::write(ws.root.join("long.txt"), (0..100).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n")).unwrap();
        let fs = FileSystem::new();
        let r = fs.read(&ws, "long.txt", Some(10), Some(5)).unwrap();
        assert_eq!(r.total_lines, 100);
        assert!(r.truncated, "limited read should be truncated");
        let first = r.content.lines().next().unwrap();
        assert_eq!(first, "line10");
        assert_eq!(r.content.lines().count(), 5);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn read_binary_rejected() {
        let (root, ws) = setup("read_binary");
        fs::write(ws.root.join("blob.bin"), b"\x00\x01\x02").unwrap();
        let fs = FileSystem::new();
        let err = fs.read(&ws, "blob.bin", None, None).unwrap_err();
        assert!(err.to_string().contains("binary"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn write_external_modification_detected() {
        let (root, ws) = setup("write_external");
        let fs = FileSystem::new();
        let r = fs.read(&ws, "a.txt", None, None).unwrap();
        // 外部修改：磁盘 mtime 变化。
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(ws.root.join("a.txt"), "changed by external\n").unwrap();
        let err = fs.write(&ws, "a.txt", "overwrite", r.mtime_ms.into()).unwrap_err();
        assert!(err.to_string().contains("modified on disk"), "{err}");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn write_new_file_without_read_ok() {
        let (root, ws) = setup("write_new");
        let fs = FileSystem::new();
        fs.write(&ws, "new.txt", "fresh\n", None).unwrap();
        let r = fs.read(&ws, "new.txt", None, None).unwrap();
        assert_eq!(r.content, "fresh\n");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn delete_and_rename_and_move() {
        let (root, ws) = setup("delete_rename");
        let fs = FileSystem::new();
        fs.create_dir(&ws, "tmp/sub").unwrap();
        assert!(ws.root.join("tmp/sub").is_dir());
        fs.write(&ws, "tmp/sub/f.txt", "f", None).unwrap();
        fs.rename(&ws, "tmp/sub/f.txt", "tmp/sub/g.txt").unwrap();
        assert!(ws.root.join("tmp/sub/g.txt").exists());
        fs.rename(&ws, "tmp/sub/g.txt", "moved.txt").unwrap();
        assert!(ws.root.join("moved.txt").exists());
        fs.delete(&ws, &["tmp".into(), "moved.txt".into()]).unwrap();
        assert!(!ws.root.join("tmp").exists());
        assert!(!ws.root.join("moved.txt").exists());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rename_cross_workspace_escape_rejected() {
        let (root, ws) = setup("rename_escape");
        fs::write(ws.root.join("f.txt"), "x").unwrap();
        let fs = FileSystem::new();
        let err = fs.rename(&ws, "f.txt", "../out.txt").unwrap_err();
        assert!(err.to_string().contains("workspace root"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn glob_and_grep() {
        let (root, ws) = setup("glob_grep");
        let fs = FileSystem::new();
        let globs = fs.glob(&ws, "**/*.rs", None).unwrap();
        assert!(globs.iter().any(|m| m.path == "src/lib/mod.rs"));
        let hits = fs
            .grep(&ws, "mod", None, false, false, Some("*.rs"), 0)
            .unwrap();
        assert!(hits.iter().any(|m| m.path == "src/lib/mod.rs" && m.line == 1));
        let nohit = fs.grep(&ws, "zzz_none", None, false, false, None, 0).unwrap();
        assert!(nohit.is_empty());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn grep_case_and_context() {
        let (root, ws) = setup("grep_case");
        fs::write(ws.root.join("c.txt"), "Alpha\nbeta\nGamma\n").unwrap();
        let fs = FileSystem::new();
        // 默认大小写不敏感。
        let hits = fs.grep(&ws, "gamma", None, false, false, None, 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 3);
        assert_eq!(hits[0].context_before, vec!["beta".to_string()]);
        // 大小写敏感。
        let hits = fs.grep(&ws, "alpha", None, true, false, None, 0).unwrap();
        assert!(hits.is_empty());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn info_exists_and_missing() {
        let (root, ws) = setup("info");
        let fs = FileSystem::new();
        let info = fs.info(&ws, "a.txt").unwrap();
        assert!(info.exists);
        assert!(!info.is_dir);
        assert!(!info.is_binary);
        let missing = fs.info(&ws, "nope.txt").unwrap();
        assert!(!missing.exists);
        fs::remove_dir_all(&root).ok();
    }
}

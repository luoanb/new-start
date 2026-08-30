//! 索引构建（mtime 增量）+ 块级检索（SQLite FTS5 + bm25 + 块类型加权）。
//!
//! 索引按项目（workspace）独立存储：`<index_root>/<sha256(root)[..16]>/search.db`。
//! 检索是 FTS5 块级关键词召回：英文走 content 前缀匹配（unicode61 已拆 `_`），
//! 中文走 `cjk` 列的 2-gram 子串匹配；跨 token 短查询 AND、长查询降级 OR。
//! （embedding 向量通道留 v2，chunk 表可增量加列）。

use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

use crate::core::error::{AppError, AppResult};
use crate::fileops::fs::is_ignored;
use crate::fileops::workspace::WorkspaceEntry;

use super::chunk::{CodeChunk, SearchBlock, SemanticSearchResult};
use super::indexer::Chunker;

const DB_FILE: &str = "search.db";
const SCHEMA_VERSION: i64 = 2;
/// FTS5 虚拟表定义（v2：新增 `cjk` 中文 2-gram 列，unicode61 分词）。
const FTS5_SCHEMA: &str = "CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
           id UNINDEXED, path UNINDEXED, start_line UNINDEXED,
           end_line UNINDEXED, block_type UNINDEXED, content, cjk
         )";
/// 跨 token 组合阈值：≥该数量（通常为同义词枚举）降级 OR 宽松召回。
const OR_JOIN_MIN_TOKENS: usize = 4;
/// 单文件大小上限（超出跳过索引，与分块器保持一致）。
const MAX_FILE_BYTES: usize = 512 * 1024;
/// 检索默认/上限。
const DEFAULT_TOP_K: usize = 10;
const MAX_TOP_K: usize = 20;
/// 内容摘要截断（对外契约）。
const MAX_SNIPPET_CHARS: usize = 400;
/// 加权重排前多取的行数（抵消块类型加权对排名的扰动）。
const RANK_PREFETCH_MULT: usize = 4;

/// 索引构建统计（供搜索命令/工具返回给调用方）。
pub struct IndexStats {
    pub indexed_blocks: usize,
    pub index_duration_ms: u64,
}

/// 索引器：懒构建 + mtime/size 增量。
pub struct Indexer;

impl Indexer {
    /// 确保 active workspace 的索引就绪（首次全量构建，之后增量更新）。
    pub fn ensure_index(index_root: &Path, ws: &WorkspaceEntry) -> AppResult<IndexStats> {
        let started = Instant::now();
        let db_path = index_dir_for(index_root, &ws.root).join(DB_FILE);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::RuntimeError(format!("create search index dir: {e}"))
            })?;
        }
        let mut conn = Connection::open(&db_path)
            .map_err(|e| AppError::StorageError(format!("open search index: {e}")))?;
        init_schema(&mut conn)?;

        // 1. 读已知文件（path → (mtime_ms, size)）。
        let mut known: HashMap<String, (i64, i64)> = HashMap::new();
        {
            let mut stmt = conn
                .prepare("SELECT path, mtime_ms, size FROM files")
                .map_err(db_err)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))
                })
                .map_err(db_err)?;
            for row in rows {
                if let Ok((path, info)) = row {
                    known.insert(path, info);
                }
            }
        }

        // 2. 遍历工作区：目录按 ignore 剪枝，文件按 mtime/size 判定是否需重建。
        let mut visited: HashSet<String> = HashSet::new();
        let mut changed: Vec<(String, std::fs::Metadata, PathBuf)> = Vec::new();
        for entry in WalkDir::new(&ws.root)
            .into_iter()
            .filter_entry(|e| {
                if !e.file_type().is_dir() {
                    return true;
                }
                let rel = rel_of(&ws.root, e.path());
                rel.is_empty() || !is_ignored(&rel, &ws.ignore)
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!(error = %e, "semantic search: walkdir entry skipped");
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = rel_of(&ws.root, entry.path());
            if rel.is_empty() || is_ignored(&rel, &ws.ignore) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.len() as usize > MAX_FILE_BYTES {
                visited.insert(rel);
                continue;
            }
            let info = (mtime_ms(&meta), meta.len() as i64);
            if known.get(&rel).copied() == Some(info) {
                visited.insert(rel);
                continue;
            }
            changed.push((rel, meta, entry.path().to_path_buf()));
        }

        // 3. 清理磁盘上已消失的文件索引。
        for rel in known.keys() {
            if !visited.contains(rel) && !changed.iter().any(|(r, _, _)| r == rel) {
                delete_file_chunks(&mut conn, rel)?;
            }
        }

        // 4. 重建变更文件（事务批处理）。
        if !changed.is_empty() {
            let tx = conn
                .transaction()
                .map_err(|e| AppError::StorageError(format!("search index tx: {e}")))?;
            for (rel, meta, abs) in &changed {
                let Ok(content) = std::fs::read_to_string(abs) else { continue };
                if content.as_bytes().contains(&0) {
                    // 二进制文件跳过索引；登记到 files 表避免每次重扫。
                    upsert_file(&tx, rel, mtime_ms(meta), meta.len() as i64)?;
                    visited.insert(rel.clone());
                    continue;
                }
                delete_file_chunks_tx(&tx, rel)?;
                let chunks = Chunker::chunk(rel, &content);
                insert_chunks_tx(&tx, rel, &chunks)?;
                upsert_file(&tx, rel, mtime_ms(meta), meta.len() as i64)?;
                visited.insert(rel.clone());
            }
            tx.commit()
                .map_err(|e| AppError::StorageError(format!("search index commit: {e}")))?;
        }

        let indexed_blocks: usize = conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
            .map_err(db_err)?;
        Ok(IndexStats {
            indexed_blocks,
            index_duration_ms: started.elapsed().as_millis() as u64,
        })
    }
}

/// 检索器：query 预处理 → FTS5 MATCH → bm25 + 块类型加权 → 截断。
pub struct Retriever;

impl Retriever {
    pub fn search(
        index_root: &Path,
        ws: &WorkspaceEntry,
        query: &str,
        top_k: Option<usize>,
        path: Option<&str>,
    ) -> AppResult<SemanticSearchResult> {
        let stats = Indexer::ensure_index(index_root, ws)?;
        let top_k = top_k.unwrap_or(DEFAULT_TOP_K).clamp(1, MAX_TOP_K);

        let tokens = normalize_query(query);
        if tokens.is_empty() {
            return Err(AppError::InvalidInput(
                "query must contain at least one searchable word".into(),
            ));
        }
        // FTS5 column filter：
        // - 英文 token → content 前缀匹配（`"stop"*` 命中 `stop_session`/`stopAll`；unicode61 已把 `_` 拆词）。
        // - 中文 token → 按 2-gram 展开到 cjk 列 OR 组合（`中断` 命中注释中间的「会话中断」，子串级召回）；
        //   单字中文无 bigram 时回退 content 前缀匹配。
        // - 跨 token 组合：≤3 词 AND 保精度；≥4 词（通常为同义词枚举）降级 OR 宽松召回，靠 bm25 收敛。
        let mut clauses: Vec<String> = Vec::with_capacity(tokens.len());
        for t in &tokens {
            let clause = if is_cjk(t) {
                let mut grams = query_bigrams(t);
                grams.sort();
                grams.dedup();
                if grams.is_empty() {
                    format!("content:\"{t}\"*")
                } else {
                    grams
                        .iter()
                        .map(|g| format!("cjk:\"{g}\""))
                        .collect::<Vec<_>>()
                        .join(" OR ")
                }
            } else {
                format!("content:\"{t}\"*")
            };
            clauses.push(format!("({clause})"));
        }
        let joiner = if clauses.len() >= OR_JOIN_MIN_TOKENS {
            " OR "
        } else {
            " AND "
        };
        let match_expr = clauses.join(joiner);

        let db_path = index_dir_for(index_root, &ws.root).join(DB_FILE);
        let conn = Connection::open(&db_path)
            .map_err(|e| AppError::StorageError(format!("open search index: {e}")))?;

        let path_filter = path.map(str::trim).filter(|p| !p.is_empty()).unwrap_or("");
        let prefix = if path_filter.is_empty() {
            String::new()
        } else {
            format!("{path_filter}/%")
        };
        let limit = (top_k * RANK_PREFETCH_MULT) as i64;

        let sql = "SELECT path, start_line, end_line, block_type, content, \
                   (bm25(chunks_fts, 0, 0, 0, 0, 0, 5.0, 2.0) + \
                     CASE block_type \
                       WHEN 'impl' THEN 0.6 WHEN 'trait' THEN 0.6 WHEN 'interface' THEN 0.6 \
                       WHEN 'function' THEN 0.3 WHEN 'struct' THEN 0.3 WHEN 'class' THEN 0.3 WHEN 'enum' THEN 0.3 \
                       ELSE 0.0 END) AS score \
                   FROM chunks_fts \
                   WHERE chunks_fts MATCH ?1 \
                     AND (?2 = '' OR path = ?2 OR path LIKE ?3) \
                   ORDER BY score DESC \
                   LIMIT ?4";
        let mut stmt = conn.prepare(sql).map_err(db_err)?;
        let rows = stmt
            .query_map(params![match_expr, path_filter, prefix, limit], |r| {
                Ok(SearchBlock {
                    path: r.get(0)?,
                    start_line: r.get::<_, i64>(1)? as usize,
                    end_line: r.get::<_, i64>(2)? as usize,
                    block_type: r.get(3)?,
                    content: r.get(4)?,
                    score: r.get(5)?,
                })
            })
            .map_err(db_err)?;

        let mut results: Vec<SearchBlock> = Vec::new();
        for row in rows {
            let Ok(mut block) = row else { continue };
            block.content = truncate(&block.content, MAX_SNIPPET_CHARS);
            results.push(block);
        }
        results.truncate(top_k);

        Ok(SemanticSearchResult {
            results,
            indexed_blocks: stats.indexed_blocks,
            index_duration_ms: stats.index_duration_ms,
        })
    }
}

/// 索引根目录：`<index_root>/<sha256(root)[..16]>`。
fn index_dir_for(index_root: &Path, ws_root: &Path) -> PathBuf {
    let canonical = ws_root.canonicalize().unwrap_or_else(|_| ws_root.to_path_buf());
    let digest = sha256_prefix(&canonical.to_string_lossy(), 16);
    index_root.join(digest)
}

/// sha256 前 `len` 个十六进制字符（降低目录冲突、保持可读）。
fn sha256_prefix(input: &str, len: usize) -> String {
    use std::fmt::Write as _;
    let digest = <sha2::Sha256 as sha2::Digest>::digest(input.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest.iter() {
        let _ = write!(hex, "{byte:02x}");
    }
    hex.truncate(len);
    hex
}

fn init_schema(conn: &mut Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS files (
           path TEXT PRIMARY KEY,
           mtime_ms INTEGER NOT NULL,
           size INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS chunks (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           path TEXT NOT NULL,
           start_line INTEGER NOT NULL,
           end_line INTEGER NOT NULL,
           block_type TEXT NOT NULL,
           content TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_chunks_path ON chunks(path);",
    )
    .map_err(db_err)?;
    let ver: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
        .optional()
        .map_err(db_err)?;
    if ver.as_deref() != Some(&SCHEMA_VERSION.to_string()) {
        // 版本升级：重建 FTS 表（新增 cjk 列）并清空索引登记，触发下一轮全量重建
        // （增量扫描以 files 表为准，清空后所有文件都会视为变更重新分块）。
        conn.execute_batch(&format!(
            "DROP TABLE IF EXISTS chunks_fts;
             {FTS5_SCHEMA};
             DELETE FROM files;
             DELETE FROM chunks;"
        ))
        .map_err(db_err)?;
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )
        .map_err(db_err)?;
    } else {
        // 全新库幂等创建（已存在同版本则跳过）。
        conn.execute_batch(FTS5_SCHEMA).map_err(db_err)?;
    }
    Ok(())
}

fn delete_file_chunks(conn: &mut Connection, rel: &str) -> AppResult<()> {
    conn.execute("DELETE FROM chunks WHERE path = ?1", params![rel]).map_err(db_err)?;
    conn.execute("DELETE FROM chunks_fts WHERE path = ?1", params![rel]).map_err(db_err)?;
    conn.execute("DELETE FROM files WHERE path = ?1", params![rel]).map_err(db_err)?;
    Ok(())
}

fn delete_file_chunks_tx(tx: &rusqlite::Transaction<'_>, rel: &str) -> AppResult<()> {
    tx.execute("DELETE FROM chunks WHERE path = ?1", params![rel]).map_err(db_err)?;
    tx.execute("DELETE FROM chunks_fts WHERE path = ?1", params![rel]).map_err(db_err)?;
    Ok(())
}

fn insert_chunks_tx(
    tx: &rusqlite::Transaction<'_>,
    rel: &str,
    chunks: &[CodeChunk],
) -> AppResult<()> {
    let mut stmt = tx
        .prepare("INSERT INTO chunks (path, start_line, end_line, block_type, content) VALUES (?1, ?2, ?3, ?4, ?5)")
        .map_err(db_err)?;
    let mut fts = tx
        .prepare("INSERT INTO chunks_fts (id, path, start_line, end_line, block_type, content, cjk) VALUES (last_insert_rowid(), ?1, ?2, ?3, ?4, ?5, ?6)")
        .map_err(db_err)?;
    for chunk in chunks {
        stmt.execute(params![
            rel,
            chunk.start_line as i64,
            chunk.end_line as i64,
            chunk.block_type.as_str(),
            chunk.content,
        ])
        .map_err(db_err)?;
        let cjk = cjk_bigrams(&chunk.content);
        fts.execute(params![
            rel,
            chunk.start_line as i64,
            chunk.end_line as i64,
            chunk.block_type.as_str(),
            chunk.content,
            cjk,
        ])
        .map_err(db_err)?;
    }
    Ok(())
}

fn upsert_file(tx: &rusqlite::Transaction<'_>, rel: &str, mt: i64, size: i64) -> AppResult<()> {
    tx.execute(
        "INSERT OR REPLACE INTO files (path, mtime_ms, size) VALUES (?1, ?2, ?3)",
        params![rel, mt, size],
    )
    .map_err(db_err)?;
    Ok(())
}

/// query 预处理：小写 + 拆词（保留字母数字、下划线与 CJK，对齐 unicode61 拆词语义），
/// 过滤空串与单字符噪声。中文关键字不再被当作分隔符丢弃。
fn normalize_query(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.len() >= 2)
        .map(str::to_string)
        .collect()
}

/// 是否含 CJK 字符（汉字）：CJK 词无空格分隔，FTS5 unicode61 按连续字母合成 token，
/// 用前缀匹配弥补「短词命中长词」的精确匹配局限。
fn is_cjk(s: &str) -> bool {
    s.chars().any(|c| !c.is_ascii() && c.is_alphabetic())
}

/// CJK 字符判定（与 `is_cjk` 同谓词）：非 ASCII 字母（中文/日文/韩文等）。
fn is_cjk_char(c: char) -> bool {
    !c.is_ascii() && c.is_alphabetic()
}

/// 提取 content 中所有连续 CJK 字段的重叠 2-gram，空格分隔（供 `cjk` 列索引）。
/// 例：`对话中断时的处理逻辑` → `对话 话中 中断 断时 时的 的处 处理 理逻 逻辑`。
fn cjk_bigrams(content: &str) -> String {
    let mut out = String::new();
    let mut run: Vec<char> = Vec::new();
    for c in content.chars() {
        if is_cjk_char(c) {
            run.push(c);
        } else {
            push_bigrams(&run, &mut out);
            run.clear();
        }
    }
    push_bigrams(&run, &mut out);
    out
}

/// 查询词的 CJK 2-gram 集合（子串级召回，任一 bigram 命中即算该词命中）。
/// 例：`对话中断` → [对话, 话中, 中断]；单字词返回空（调用方回退前缀匹配）。
fn query_bigrams(token: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<char> = Vec::new();
    for c in token.chars() {
        if is_cjk_char(c) {
            run.push(c);
        } else {
            collect_bigrams(&run, &mut out);
            run.clear();
        }
    }
    collect_bigrams(&run, &mut out);
    out
}

fn push_bigrams(run: &[char], out: &mut String) {
    for w in run.windows(2) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push(w[0]);
        out.push(w[1]);
    }
}

fn collect_bigrams(run: &[char], out: &mut Vec<String>) {
    for w in run.windows(2) {
        let mut g = String::with_capacity(2);
        g.push(w[0]);
        g.push(w[1]);
        out.push(g);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push_str("…");
        out
    }
}

fn rel_of(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root)
        .map(|p| {
            p.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn db_err(e: rusqlite::Error) -> AppError {
    AppError::StorageError(format!("search index db: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fileops::workspace::WorkspaceStore;
    use std::sync::Arc;

    fn setup(name: &str) -> (PathBuf, PathBuf, WorkspaceEntry) {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let root = std::env::temp_dir().join(format!("pulsar-search-{name}-{ms}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let ws = root.join("proj");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::write(
            ws.join("src/main.rs"),
            "fn helper() -> i32 { 42 }\n\nfn main() {\n    let _ = helper();\n}\n",
        )
        .unwrap();
        std::fs::write(
            ws.join("src/auth.rs"),
            "pub fn login(user: &str, pass: &str) -> bool {\n    user == \"admin\" && pass == \"secret\"\n}\n",
        )
        .unwrap();
        // 被 ignore 的文件不应进索引。
        std::fs::create_dir_all(ws.join("node_modules")).unwrap();
        std::fs::write(ws.join("node_modules/dep.js"), "function hidden() { return 1; }\n").unwrap();
        let store = Arc::new(WorkspaceStore::new(&root).unwrap());
        let view = store.add(ws.to_str().unwrap()).unwrap();
        let entry = view.workspaces[0].clone();
        (root, ws, entry)
    }

    fn index_root(base: &Path) -> PathBuf {
        base.join("index")
    }

    #[test]
    fn build_and_incremental_update() {
        let (base, ws, entry) = setup("incr");
        let root = index_root(&base);
        let stats = Indexer::ensure_index(&root, &entry).unwrap();
        assert!(stats.indexed_blocks >= 3, "两个函数 + 文件兜底: {}", stats.indexed_blocks);

        // 未变：再跑一次走增量，块数不变且耗时归零分支（仍返回 stats）。
        let stats2 = Indexer::ensure_index(&root, &entry).unwrap();
        assert_eq!(stats2.indexed_blocks, stats.indexed_blocks);

        // 修改文件 → 块数变化。
        std::fs::write(
            ws.join("src/main.rs"),
            "fn helper() -> i32 { 7 }\n\nfn another() {}\n\nfn main() { another(); }\n",
        )
        .unwrap();
        let stats3 = Indexer::ensure_index(&root, &entry).unwrap();
        assert_eq!(stats3.indexed_blocks, stats.indexed_blocks + 1);

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn ignore_rules_excluded() {
        let (base, _ws, entry) = setup("ignore");
        let root = index_root(&base);
        let stats = Indexer::ensure_index(&root, &entry).unwrap();
        assert!(stats.indexed_blocks > 0);
        let db = index_dir_for(&root, &entry.root).join(DB_FILE);
        let conn = Connection::open(db).unwrap();
        let hidden: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks WHERE path LIKE 'node_modules%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hidden, 0, "node_modules 不应进索引");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn search_returns_relevant_blocks() {
        let (base, _ws, entry) = setup("search");
        let root = index_root(&base);
        let result = Retriever::search(&root, &entry, "login", None, None).unwrap();
        assert!(!result.results.is_empty());
        let top = &result.results[0];
        assert_eq!(top.path, "src/auth.rs");
        assert!(top.content.contains("login"));

        // path 过滤。
        let filtered =
            Retriever::search(&root, &entry, "helper", None, Some("src/main.rs")).unwrap();
        assert!(
            !filtered.results.is_empty()
                && filtered.results.iter().all(|b| b.path == "src/main.rs"),
            "path 过滤后应只剩 main.rs: {:?}",
            filtered.results
        );

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn empty_query_rejected() {
        let (base, _ws, entry) = setup("empty");
        let root = index_root(&base);
        let err = Retriever::search(&root, &entry, "a!", None, None).unwrap_err();
        assert!(err.to_string().contains("query"));
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn cjk_query_matches_chinese_content() {
        let (base, ws, entry) = setup("cjk");
        // 中文出现在函数块内容中（字符串字面量），验证 CJK 关键词可检索。
        std::fs::write(
            ws.join("src/semantic.rs"),
            "pub fn tag() -> &'static str { \"语义搜索\" }\n",
        )
        .unwrap();
        let root = index_root(&base);
        let _ = Indexer::ensure_index(&root, &entry).unwrap();

        // 完整中文词命中。
        let full = Retriever::search(&root, &entry, "语义搜索", None, None).unwrap();
        assert!(
            full.results.iter().any(|b| b.path == "src/semantic.rs"),
            "完整词应命中 semantic.rs: {:?}",
            full.results
        );

        // 中文短词经前缀匹配命中长词（「语义」→「语义搜索」）。
        let short = Retriever::search(&root, &entry, "语义", None, None).unwrap();
        assert!(
            short.results.iter().any(|b| b.path == "src/semantic.rs"),
            "短词前缀应命中 semantic.rs: {:?}",
            short.results
        );

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn workspaces_are_isolated() {
        let (base, _ws, entry) = setup("iso");
        let root = index_root(&base);
        Indexer::ensure_index(&root, &entry).unwrap();
        // 第二个工作区（不同 hash 目录）。
        let ws2 = base.join("proj2");
        std::fs::create_dir_all(&ws2).unwrap();
        std::fs::write(ws2.join("x.rs"), "fn other() {}\n").unwrap();
        let store = Arc::new(WorkspaceStore::new(&base).unwrap());
        let view = store.add(ws2.to_str().unwrap()).unwrap();
        let entry2 = view
            .workspaces
            .iter()
            .find(|w| w.root == ws2.canonicalize().unwrap())
            .cloned()
            .unwrap();
        Indexer::ensure_index(&root, &entry2).unwrap();
        let r = Retriever::search(&root, &entry, "login", None, None).unwrap();
        assert!(r.results.iter().all(|b| b.path == "src/auth.rs"));
        let r2 = Retriever::search(&root, &entry2, "login", None, None).unwrap();
        assert!(r2.results.is_empty(), "跨工作区不应命中");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn cjk_mid_comment_substring_hits() {
        let (base, ws, entry) = setup("cjkmid");
        std::fs::write(
            ws.join("src/stop.rs"),
            "fn handle() -> u32 {\n    // 处理会话中断时的逻辑\n    1\n}\n",
        )
        .unwrap();
        let root = index_root(&base);
        let _ = Indexer::ensure_index(&root, &entry).unwrap();
        // 「中断」在函数块内注释中间（token 以「处理」开头），前缀匹配不命中，2-gram 子串命中。
        let r = Retriever::search(&root, &entry, "中断", None, None).unwrap();
        assert!(
            r.results.iter().any(|b| b.path == "src/stop.rs"),
            "注释中间的「中断」应子串命中: {:?}",
            r.results
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn english_prefix_matches_camel_case_identifier() {
        let (base, ws, entry) = setup("camel");
        std::fs::write(ws.join("src/cmd.rs"), "fn stopAllSessions() {}\n").unwrap();
        let root = index_root(&base);
        let _ = Indexer::ensure_index(&root, &entry).unwrap();
        // unicode61 不拆驼峰（stopall... 单 token），前缀 `"stop"*` 命中。
        let r = Retriever::search(&root, &entry, "stop", None, None).unwrap();
        assert!(
            r.results.iter().any(|b| b.path == "src/cmd.rs"),
            "驼峰标识符 stopAllSessions 应被 stop 前缀命中: {:?}",
            r.results
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn long_query_or_recalls_partial_match() {
        let (base, ws, entry) = setup("orquery");
        std::fs::write(
            ws.join("src/abort.rs"),
            "fn abort_stream() {\n    // 中断处理\n}\n",
        )
        .unwrap();
        let root = index_root(&base);
        let _ = Indexer::ensure_index(&root, &entry).unwrap();
        // 5 个词只有「中断」命中：长查询（≥4 词）降级 OR，不应返回空。
        let r = Retriever::search(&root, &entry, "对话 中止 会话 中断 重启", None, None).unwrap();
        assert!(
            !r.results.is_empty(),
            "长查询 OR 应召回部分命中的块: {:?}",
            r.results
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn single_char_cjk_falls_back_to_content_prefix() {
        let (base, ws, entry) = setup("single");
        std::fs::write(ws.join("src/stop.rs"), "fn h() {\n    // 停止会话\n}\n").unwrap();
        let root = index_root(&base);
        let _ = Indexer::ensure_index(&root, &entry).unwrap();
        // 单字「停」无 bigram，回退 content 前缀匹配「停止」。
        let r = Retriever::search(&root, &entry, "停", None, None).unwrap();
        assert!(
            r.results.iter().any(|b| b.path == "src/stop.rs"),
            "单字中文回退前缀匹配: {:?}",
            r.results
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn schema_v1_migrates_and_reindexes() {
        let (base, _ws, entry) = setup("migrate");
        let root = index_root(&base);
        let _ = Indexer::ensure_index(&root, &entry).unwrap();
        let db = index_dir_for(&root, &entry.root).join(DB_FILE);
        // 模拟旧版库：无 cjk 列的 fts + 版本号回退 1。
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "DROP TABLE chunks_fts;
             CREATE VIRTUAL TABLE chunks_fts USING fts5(
               id UNINDEXED, path UNINDEXED, start_line UNINDEXED,
               end_line UNINDEXED, block_type UNINDEXED, content
             );
             INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '1');",
        )
        .unwrap();
        drop(conn);
        // 再 ensure：应检测版本不符 → 重建带 cjk 的 fts 并全量重索引。
        let stats = Indexer::ensure_index(&root, &entry).unwrap();
        assert!(stats.indexed_blocks >= 3, "迁移后应全量重建索引");
        let conn = Connection::open(&db).unwrap();
        let ver: String = conn
            .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ver, "2");
        drop(conn);
        let r = Retriever::search(&root, &entry, "login", None, None).unwrap();
        assert!(!r.results.is_empty(), "迁移后检索可用");
        std::fs::remove_dir_all(&base).ok();
    }
}

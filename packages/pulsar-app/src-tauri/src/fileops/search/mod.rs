//! 语义搜索：块级检索子模块（文件管理领域下的文件/工作区能力）。
//!
//! - `chunk`：代码块数据模型（BlockType / CodeChunk / 对外契约）。
//! - `indexer`：tree-sitter 语法感知分块（未知语言回退启发式）。
//! - `retriever`：SQLite FTS5 索引（mtime 增量）+ bm25 块级检索（含块类型加权）。
//! - `tools`：`semantic_search` AI 原生工具（native 通道，insert 门禁）。
//!
//! 索引按项目（workspace）独立存储于应用数据目录（`<index_root>/<root_hash>/`），
//! 不写入用户项目目录；embedding 向量通道留 v2（chunk 表可增量加列）。

pub mod chunk;
pub mod indexer;
pub mod retriever;
pub mod tools;

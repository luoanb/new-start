//! 文件管理领域模块（独立于 core，自包含工作区边界与文件操作）。
//!
//! - `workspace`：可配置工作区存储（workspaces.json）+ `resolve_in_workspace` 越界护栏
//! - `fs`：文件操作层（list/read/write/create_dir/delete/rename/move/glob/grep/info + 已读清单）
//! - `fs_tools`：AI 原生文件工具（与前端 UI 共用同一护栏与已读清单）

pub mod fs;
pub mod fs_tools;
pub mod gitops;
pub mod search;
pub mod workspace;

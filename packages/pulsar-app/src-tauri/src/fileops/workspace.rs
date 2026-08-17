//! 可配置工作区存储：`workspaces.json`。
//!
//! 每个工作区条目独立携带边界（canonicalize 后的 root）与文件树过滤规则
//! （ignore，glob/前缀语义）。AI 工具与前端 UI 共用同一工作区集合与
//! 越界护栏（`resolve_in_workspace`），保证两者看到/操作同一份文件系统。

use crate::core::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// 数据文件名（与 mcp_servers.json / dynamic_tools.json 并列于存储根）。
const WORKSPACES_FILE: &str = "workspaces.json";

/// 单个工作区条目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceEntry {
    /// 稳定 id：由 canonicalized root 派生，跨重启保留；同目录重复添加自动去重。
    pub id: String,
    /// 展示名（默认取根目录名）。
    pub name: String,
    /// 规范化后的绝对路径（canonicalize）。
    pub root: PathBuf,
    /// 该工作区文件树过滤规则（glob/前缀语义，可编辑）。
    #[serde(default)]
    pub ignore: Vec<String>,
    /// 毫秒时间戳。
    pub created_at: i64,
}

/// 前端读取/写入形状。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceView {
    pub workspaces: Vec<WorkspaceEntry>,
    pub active_id: Option<String>,
}

/// 新工作区默认过滤规则（用户可改）。
pub fn default_ignore() -> Vec<String> {
    vec![".git", "node_modules", "target", "dist", ".pulsar", ".DS_Store"]
        .into_iter()
        .map(String::from)
        .collect()
}

/// 当前 Unix 毫秒时间戳（i64，供 created_at 使用）。
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis() as i64
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WorkspaceState {
    #[serde(default)]
    workspaces: Vec<WorkspaceEntry>,
    #[serde(default)]
    active_id: Option<String>,
}

/// 工作区存储：内存态（RwLock 包裹）+ workspaces.json 持久化。
#[derive(Debug)]
pub struct WorkspaceStore {
    path: PathBuf,
    state: RwLock<WorkspaceState>,
}

fn id_for_root(root: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    root.hash(&mut hasher);
    format!("ws-{:x}", hasher.finish())
}

fn atomic_write(path: &Path, content: &str) -> AppResult<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

impl WorkspaceStore {
    /// 加载（缺失 → 默认空）；非法 JSON 返回可读错误。
    pub fn new(storage_root: &Path) -> AppResult<Self> {
        let path = storage_root.join(WORKSPACES_FILE);
        let state = if path.exists() {
            let text = fs::read_to_string(&path)?;
            serde_json::from_str::<WorkspaceState>(&text)
                .map_err(|e| AppError::StorageError(format!("workspaces.json invalid: {e}")))?
        } else {
            WorkspaceState::default()
        };
        Ok(Self {
            path,
            state: RwLock::new(state),
        })
    }

    fn persist(&self, state: &WorkspaceState) -> AppResult<()> {
        let text = serde_json::to_string_pretty(state)?;
        atomic_write(&self.path, &text)?;
        Ok(())
    }

    /// 全量视图（前端读取形状）。
    pub fn view(&self) -> AppResult<WorkspaceView> {
        let guard = self
            .state
            .read()
            .map_err(|e| AppError::RuntimeError(format!("workspace lock: {e}")))?;
        Ok(WorkspaceView {
            workspaces: guard.workspaces.clone(),
            active_id: guard.active_id.clone(),
        })
    }

    pub fn list(&self) -> AppResult<Vec<WorkspaceEntry>> {
        Ok(self.view()?.workspaces)
    }

    /// 当前 active 工作区（无则 None）。
    pub fn active(&self) -> AppResult<Option<WorkspaceEntry>> {
        let view = self.view()?;
        Ok(view
            .active_id
            .and_then(|id| view.workspaces.iter().find(|w| w.id == id).cloned()))
    }

    /// 按 id 取工作区。
    pub fn get(&self, id: &str) -> AppResult<Option<WorkspaceEntry>> {
        let view = self.view()?;
        Ok(view.workspaces.iter().find(|w| w.id == id).cloned())
    }

    /// 添加工作区：校验目录存在、canonicalize、去重；返回新视图。
    pub fn add(&self, root: &str) -> AppResult<WorkspaceView> {
        let raw = PathBuf::from(root.trim());
        if raw.as_os_str().is_empty() {
            return Err(AppError::InvalidInput("workspace root cannot be empty".into()));
        }
        if !raw.exists() {
            return Err(AppError::InvalidInput(format!(
                "workspace root does not exist: {}",
                raw.display()
            )));
        }
        if !raw.is_dir() {
            return Err(AppError::InvalidInput(format!(
                "workspace root is not a directory: {}",
                raw.display()
            )));
        }
        let canonical = raw.canonicalize()?;
        let id = id_for_root(&canonical);

        let mut guard = self
            .state
            .write()
            .map_err(|e| AppError::RuntimeError(format!("workspace lock: {e}")))?;
        if guard.workspaces.iter().any(|w| w.id == id) {
            return Err(AppError::InvalidInput(
                "workspace already exists".into(),
            ));
        }
        let name = canonical
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| canonical.display().to_string());
        guard.workspaces.push(WorkspaceEntry {
            id: id.clone(),
            name,
            root: canonical,
            ignore: default_ignore(),
            created_at: now_ms(),
        });
        if guard.active_id.is_none() {
            guard.active_id = Some(id);
        }
        self.persist(&guard)?;
        Ok(WorkspaceView {
            workspaces: guard.workspaces.clone(),
            active_id: guard.active_id.clone(),
        })
    }

    /// 移除工作区条目（不删磁盘目录）；active 被删则清除 active。
    pub fn remove(&self, id: &str) -> AppResult<WorkspaceView> {
        let mut guard = self
            .state
            .write()
            .map_err(|e| AppError::RuntimeError(format!("workspace lock: {e}")))?;
        let before = guard.workspaces.len();
        guard.workspaces.retain(|w| w.id != id);
        if guard.workspaces.len() == before {
            return Err(AppError::InvalidInput(format!("workspace not found: {id}")));
        }
        if guard.active_id.as_deref() == Some(id) {
            guard.active_id = None;
        }
        self.persist(&guard)?;
        Ok(WorkspaceView {
            workspaces: guard.workspaces.clone(),
            active_id: guard.active_id.clone(),
        })
    }

    /// 设置 active；校验存在。
    pub fn set_active(&self, id: &str) -> AppResult<WorkspaceView> {
        let mut guard = self
            .state
            .write()
            .map_err(|e| AppError::RuntimeError(format!("workspace lock: {e}")))?;
        if !guard.workspaces.iter().any(|w| w.id == id) {
            return Err(AppError::InvalidInput(format!("workspace not found: {id}")));
        }
        guard.active_id = Some(id.to_string());
        self.persist(&guard)?;
        Ok(WorkspaceView {
            workspaces: guard.workspaces.clone(),
            active_id: guard.active_id.clone(),
        })
    }

    /// 更新某工作区的 ignore 过滤规则。
    pub fn update_ignore(&self, id: &str, ignore: Vec<String>) -> AppResult<WorkspaceView> {
        let mut guard = self
            .state
            .write()
            .map_err(|e| AppError::RuntimeError(format!("workspace lock: {e}")))?;
        let entry = guard
            .workspaces
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| AppError::InvalidInput(format!("workspace not found: {id}")))?;
        entry.ignore = ignore;
        self.persist(&guard)?;
        Ok(WorkspaceView {
            workspaces: guard.workspaces.clone(),
            active_id: guard.active_id.clone(),
        })
    }

    /// 越界护栏：把工作区内相对路径解析为绝对路径。
    ///
    /// - `root` 须为 canonicalized 后的工作区根；
    /// - 拒绝绝对路径、含 `..` 的路径；
    /// - 对存在的路径 canonicalize 后做前缀校验（符号链接逃逸 → 拒绝）；
    /// - 对不存在的目标（写操作），解析最近存在的祖先再拼接剩余段并校验前缀。
    pub fn resolve_in_workspace(root: &Path, rel: &str) -> AppResult<PathBuf> {
        let root = root
            .canonicalize()
            .map_err(|e| AppError::InvalidInput(format!("workspace root not accessible: {e}")))?;
        let rel = rel.trim();
        if rel.is_empty() {
            return Ok(root);
        }
        if Path::new(rel).is_absolute() {
            return Err(AppError::InvalidInput(
                "path must be relative to the workspace root".into(),
            ));
        }
        for comp in rel.split(['/', '\\']) {
            if comp == ".." {
                return Err(AppError::InvalidInput(format!(
                    "path escapes workspace root: {rel}"
                )));
            }
        }
        let joined = root.join(rel);
        let resolved = match joined.canonicalize() {
            Ok(path) => path,
            // 目标尚不存在：回溯最近存在的祖先解析，再拼接缺失段。
            Err(_) => {
                let mut missing: Vec<std::ffi::OsString> = Vec::new();
                let mut cursor = joined.as_path();
                loop {
                    match cursor.canonicalize() {
                        Ok(base) => {
                            let mut path = base;
                            for seg in missing.iter().rev() {
                                path.push(seg);
                            }
                            break path;
                        }
                        Err(_) => {
                            let Some(name) = cursor.file_name() else {
                                return Err(AppError::InvalidInput(format!(
                                    "invalid path: {rel}"
                                )));
                            };
                            missing.push(name.to_os_string());
                            cursor = cursor
                                .parent()
                                .ok_or_else(|| AppError::InvalidInput(format!("invalid path: {rel}")))?;
                        }
                    }
                }
            }
        };
        if !resolved.starts_with(&root) {
            return Err(AppError::InvalidInput(format!(
                "path escapes workspace root: {rel}"
            )));
        }
        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::env::temp_dir().join(format!("pulsar-{name}-{ms}"))
    }

    fn setup(name: &str) -> (PathBuf, PathBuf) {
        let root = test_root(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let ws = root.join("proj");
        fs::create_dir_all(&ws).unwrap();
        (root, ws)
    }

    #[test]
    fn add_and_view_roundtrip() {
        let (root, ws) = setup("add_and_view");
        let store = WorkspaceStore::new(&root).unwrap();
        let view = store.add(ws.to_str().unwrap()).unwrap();
        assert_eq!(view.workspaces.len(), 1);
        assert_eq!(view.active_id.as_deref(), Some(view.workspaces[0].id.as_str()));
        // 持久化往返
        let store2 = WorkspaceStore::new(&root).unwrap();
        let view2 = store2.view().unwrap();
        assert_eq!(view2.workspaces.len(), 1);
        assert_eq!(view2.workspaces[0].root, ws.canonicalize().unwrap());
        assert!(view2.workspaces[0].ignore.len() >= 5, "default ignore");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn add_duplicate_rejected() {
        let (root, ws) = setup("add_duplicate");
        let store = WorkspaceStore::new(&root).unwrap();
        store.add(ws.to_str().unwrap()).unwrap();
        let err = store.add(ws.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("already exists"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn add_missing_root_rejected() {
        let (root, _ws) = setup("add_missing");
        let store = WorkspaceStore::new(&root).unwrap();
        let missing = root.join("nope");
        let err = store.add(missing.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn remove_and_active_cleared() {
        let (root, ws) = setup("remove_active");
        let store = WorkspaceStore::new(&root).unwrap();
        let view = store.add(ws.to_str().unwrap()).unwrap();
        let id = view.workspaces[0].id.clone();
        let view = store.remove(&id).unwrap();
        assert!(view.workspaces.is_empty());
        assert!(view.active_id.is_none());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn set_active_and_update_ignore() {
        let (root, ws) = setup("set_active");
        let store = WorkspaceStore::new(&root).unwrap();
        let view = store.add(ws.to_str().unwrap()).unwrap();
        let id = view.workspaces[0].id.clone();
        let view = store.update_ignore(&id, vec!["*.log".into()]).unwrap();
        assert_eq!(view.workspaces[0].ignore, vec!["*.log"]);
        let view = store.set_active(&id).unwrap();
        assert_eq!(view.active_id.as_deref(), Some(id.as_str()));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resolve_inside_workspace() {
        let (root, ws) = setup("resolve_inside");
        fs::create_dir_all(ws.join("src/lib")).unwrap();
        let ws_c = ws.canonicalize().unwrap();
        let p = WorkspaceStore::resolve_in_workspace(&ws_c, "src/lib/main.rs").unwrap();
        assert_eq!(p, ws_c.join("src/lib/main.rs"));
        let root_res = WorkspaceStore::resolve_in_workspace(&ws_c, "").unwrap();
        assert_eq!(root_res, ws_c);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resolve_traversal_rejected() {
        let (root, ws) = setup("resolve_traversal");
        let ws_c = ws.canonicalize().unwrap();
        for rel in ["../outside", "a/../../outside", "/etc/passwd"] {
            let err = WorkspaceStore::resolve_in_workspace(&ws_c, rel).unwrap_err();
            assert!(err.to_string().contains("workspace root") || err.to_string().contains("relative"),
                "rel {rel} should be rejected: {err}");
        }
        fs::remove_dir_all(&root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn resolve_symlink_escape_rejected() {
        use std::os::unix::fs::symlink;
        let (root, ws) = setup("resolve_symlink");
        let outside = root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        let link = ws.join("escape");
        symlink(&outside, &link).unwrap();
        let ws_c = ws.canonicalize().unwrap();
        let err = WorkspaceStore::resolve_in_workspace(&ws_c, "escape/secret.txt").unwrap_err();
        assert!(err.to_string().contains("escapes workspace root"), "symlink escape: {err}");
        fs::remove_dir_all(&root).ok();
    }
}

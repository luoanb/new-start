use std::{
    fs,
    path::{Path, PathBuf},
};

/// 当前数据目录名。
pub const STORAGE_DIR_NAME: &str = ".pulsar";
/// 旧数据目录名（改名前的 `.agent-app`），仅在首次启动时整体迁移。
const LEGACY_STORAGE_DIR_NAME: &str = ".agent-app";

/// 基于 `base` 目录解析数据目录，必要时从旧的 `.agent-app` 目录整体迁移。
pub fn resolve(base: &Path) -> PathBuf {
    let root = base.join(STORAGE_DIR_NAME);
    migrate(&root);
    root
}

/// 以当前工作目录为基座解析数据目录（CLI / TUI 入口）。
pub fn default_root() -> std::io::Result<PathBuf> {
    Ok(resolve(&std::env::current_dir()?))
}

/// 若新目录不存在而旧目录存在，将旧目录整体改名为新目录。
/// 迁移失败不阻断启动：降级为空白数据目录，下次运行重新生成。
fn migrate(root: &Path) {
    let legacy = root.with_file_name(LEGACY_STORAGE_DIR_NAME);
    if root.exists() || !legacy.exists() {
        return;
    }
    if let Err(error) = fs::rename(&legacy, root) {
        eprintln!(
            "warning: failed to migrate data from {} to {}: {error}",
            legacy.display(),
            root.display()
        );
    }
}

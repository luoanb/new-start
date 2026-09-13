use std::{
    fs,
    path::{Path, PathBuf},
};

/// 当前数据目录名。
pub const STORAGE_DIR_NAME: &str = ".pulsar";
/// 旧数据目录名（改名前的 `.agent-app`），仅在首次启动时整体迁移。
const LEGACY_STORAGE_DIR_NAME: &str = ".agent-app";

/// 数据根基座：决定 `.pulsar` 落在哪个父目录下。
///
/// 这是「数据目录放哪里」的唯一决策点（契约见 `docs/pulsar/storage.md`）：
///
/// | 入口 | 基座 | 数据根 |
/// |---|---|---|
/// | GUI 开发态（debug 构建） | [`StorageBase::Project`] | `<repo>/packages/pulsar-app/.pulsar` |
/// | GUI 发布态（release 构建） | [`StorageBase::AppData`] | `<app_data_dir>/.pulsar` |
/// | CLI / TUI / headless server | [`StorageBase::Cwd`] | `<cwd>/.pulsar` |
#[derive(Debug, Clone)]
pub enum StorageBase {
    /// 项目目录（`<repo>/packages/pulsar-app`）：开发态数据留在项目内，便于直接检查。
    ///
    /// 基座由编译期常量 `CARGO_MANIFEST_DIR` 上溯得到，**发布包中指向构建机路径**
    /// （CI 上为 `/home/runner/work/...`），因此只允许 debug 构建使用。
    Project,
    /// 系统应用数据目录（Tauri `app.path().app_data_dir()`）：发布态使用。
    AppData(PathBuf),
    /// 进程当前工作目录：CLI / TUI / headless server 使用（改 cwd 即改数据目录）。
    Cwd,
}

impl StorageBase {
    /// GUI（Tauri）入口的基座：开发态用项目目录，发布态用系统应用数据目录。
    ///
    /// 发布包不能沿用项目目录：安装位置不可写，且 `CARGO_MANIFEST_DIR` 只记录
    /// 构建机路径，会导致运行期写配置报权限错误。
    pub fn for_gui(app_data_dir: PathBuf) -> Self {
        if cfg!(debug_assertions) {
            Self::Project
        } else {
            Self::AppData(app_data_dir)
        }
    }

    /// 解析数据根（`<基座>/.pulsar`），必要时从旧的 `.agent-app` 目录整体迁移。
    ///
    /// # Errors
    ///
    /// 仅 [`StorageBase::Cwd`] 在无法取得当前工作目录时失败。
    pub fn resolve(&self) -> std::io::Result<PathBuf> {
        let base = match self {
            Self::Project => project_dir(),
            Self::AppData(dir) => dir.clone(),
            Self::Cwd => std::env::current_dir()?,
        };
        let root = base.join(STORAGE_DIR_NAME);
        migrate(&root);
        Ok(root)
    }
}

/// 项目目录：编译期 `CARGO_MANIFEST_DIR`（`packages/pulsar-app/src-tauri`）上溯一级。
fn project_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().map(Path::to_path_buf).unwrap_or(manifest)
}

/// 若新目录不存在而旧目录存在，将旧目录整体改名为新目录（同一基座内）。
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 独立临时基座（含用例名与进程号，避免并行测试互相污染）。
    fn temp_base(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("pulsar-storage-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn app_data_base_resolves_dot_pulsar_under_it() {
        let base = temp_base("appdata");
        let root = StorageBase::AppData(base.clone()).resolve().unwrap();
        assert_eq!(root, base.join(STORAGE_DIR_NAME));
    }

    #[test]
    fn project_base_resolves_under_project_dir() {
        let root = StorageBase::Project.resolve().unwrap();
        // 开发态契约：数据留在项目目录内（`<repo>/packages/pulsar-app/.pulsar`）。
        assert_eq!(root.parent().unwrap(), project_dir());
        assert!(root.ends_with(format!("pulsar-app/{STORAGE_DIR_NAME}")));
    }

    #[test]
    fn gui_base_uses_project_dir_for_debug_builds() {
        // 单元测试恒为 debug 构建：GUI 基座必须落在项目目录，而不是传入的 app data 目录。
        let base = temp_base("gui-debug");
        assert!(matches!(
            StorageBase::for_gui(base),
            StorageBase::Project
        ));
    }

    #[test]
    fn migrates_legacy_agent_app_dir() {
        let base = temp_base("legacy");
        let legacy = base.join(LEGACY_STORAGE_DIR_NAME);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("config.json"), "{}").unwrap();

        let root = StorageBase::AppData(base.clone()).resolve().unwrap();

        assert_eq!(root, base.join(STORAGE_DIR_NAME));
        assert!(root.join("config.json").is_file());
        assert!(!legacy.exists());
    }

    #[test]
    fn keeps_existing_root_and_leaves_legacy_untouched() {
        let base = temp_base("existing");
        fs::create_dir_all(base.join(STORAGE_DIR_NAME)).unwrap();
        fs::create_dir_all(base.join(LEGACY_STORAGE_DIR_NAME)).unwrap();

        let root = StorageBase::AppData(base.clone()).resolve().unwrap();

        assert!(root.is_dir());
        assert!(base.join(LEGACY_STORAGE_DIR_NAME).is_dir());
    }
}

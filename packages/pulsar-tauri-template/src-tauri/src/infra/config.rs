use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        OnceLock,
    },
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::core::error::{AppError, AppResult};

/// 统一的 `config.json` 读写入口。
///
/// 各领域模块共享此接口读写 `<storage_root>/config.json`：`read` 解析整份文件
/// （未建模的顶层键通过 `extra` 无损保留），`update` 读改写并整体写回，
/// 避免每个模块各自实现一套解析 / 序列化逻辑。
///
/// 本包为功能无关的应用骨架，不承载任何具体配置节；业务包按需在自身模块内
/// 通过 `extra` 承载或扩展结构（本类型的机制原样保留）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfigFile {
    /// 尚未建模的顶层字段原样保留，写回时不丢数据。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

pub struct ConfigStore {
    storage_root: PathBuf,
}

/// 进程级 config 写盘代数：`update` 成功后 +1（进程内单调递增，永不回退）。
/// 供依赖配置内容的进程内缓存做热失效依据：缓存键携带代数，配置更新后自然
/// miss 并重读，无需事件广播或文件 watch。
fn config_generation() -> &'static AtomicU64 {
    static GENERATION: OnceLock<AtomicU64> = OnceLock::new();
    GENERATION.get_or_init(|| AtomicU64::new(0))
}

/// 当前 config.json 写盘代数（进程内共享，各 ConfigStore 实例读取同一计数器）。
pub fn config_generation_value() -> u64 {
    config_generation().load(Ordering::Relaxed)
}

impl ConfigStore {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }

    /// 数据根目录（数据目录选择的唯一事实源），如 `debug_storage_path` 命令的读取来源。
    pub fn root(&self) -> &std::path::Path {
        &self.storage_root
    }

    pub fn path(&self) -> PathBuf {
        self.storage_root.join("config.json")
    }

    pub fn read(&self) -> AppResult<AppConfigFile> {
        let path = self.path();
        if !path.exists() {
            return Ok(AppConfigFile::default());
        }
        tracing::debug!(
            phase = crate::core::log_phase::PHASE_CONFIG_READ,
            path = %path.display(),
            "read config.json"
        );
        let content = fs::read_to_string(&path).map_err(|e| {
            AppError::StorageError(format!("Failed to read {}: {e}", path.display()))
        })?;
        serde_json::from_str(&content)
            .map_err(|e| AppError::StorageError(format!("Invalid config.json: {e}")))
    }

    /// 读改写：`f` 中修改配置，随后整体原子写回（保留未建模字段）。
    /// 写盘成功后 config 代数 +1（`config_generation_value` 随之递增）。
    pub fn update<F>(&self, f: F) -> AppResult<()>
    where
        F: FnOnce(&mut AppConfigFile),
    {
        let mut config = self.read()?;
        f(&mut config);
        let path = self.path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::StorageError(format!("Failed to create {}: {e}", parent.display()))
            })?;
        }
        let json = serde_json::to_string_pretty(&config)
            .map_err(|e| AppError::StorageError(format!("Failed to serialize config.json: {e}")))?;
        // 原子写：tmp + rename，避免写一半留下损坏文件。
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)
            .map_err(|e| AppError::StorageError(format!("Failed to write {}: {e}", tmp.display())))?;
        fs::rename(&tmp, &path)
            .map_err(|e| AppError::StorageError(format!("Failed to write {}: {e}", path.display())))?;
        // 写盘成功 → 代数 +1（依赖配置的缓存据此失效重读）。
        config_generation().fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            phase = crate::core::log_phase::PHASE_CONFIG_UPDATE,
            path = %path.display(),
            "wrote config.json"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> AppConfigFile {
        serde_json::from_str(json).expect("valid config json")
    }

    #[test]
    fn missing_file_reads_as_default() {
        let root = std::env::temp_dir().join(format!("pulsar-tauri-template-cfg-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = ConfigStore::new(root.clone());
        assert!(store.read().unwrap().extra.is_empty());
        assert_eq!(store.root(), root.as_path());
    }

    #[test]
    fn roundtrips_unmodeled_top_level_fields() {
        // 未建模顶层字段经 read/update 写回不丢失。
        let config = parse(r#"{"future_key":42}"#);
        assert_eq!(config.extra.get("future_key").and_then(|v| v.as_i64()), Some(42));
        let json = serde_json::to_string(&config).expect("serialize");
        assert!(json.contains("future_key"));
    }

    #[test]
    fn update_bumps_config_generation_and_writes_atomically() {
        // 进程级代数：update 写盘成功后 +1（缓存热失效依据）；文件真实落盘且合法。
        let root = std::env::temp_dir().join(format!(
            "pulsar-tauri-template-config-gen-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = ConfigStore::new(root.clone());
        let before = super::config_generation_value();
        store
            .update(|c| {
                c.extra.insert("future_key".into(), serde_json::json!(1));
            })
            .unwrap();
        // 并行测试可能共享进程级计数器，只断言单调递增而非精确 +1。
        assert!(super::config_generation_value() >= before + 1);
        let content = std::fs::read_to_string(store.path()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["future_key"], 1);
        // 无残留 tmp 文件。
        assert!(!store.path().with_extension("json.tmp").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::error::{AppError, AppResult};

/// 统一的 `config.json` 读写入口。
///
/// 各领域模块共享此接口读写 `.pulsar/config.json`：`read` 解析整份文件
/// （未建模的顶层键通过 `extra` 无损保留），`update` 读改写并整体写回，
/// 避免每个模块各自实现一套解析 / 序列化逻辑。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfigFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poller: Option<PollerSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub neuron: Option<NeuronSection>,
    /// 尚未建模的顶层字段原样保留，写回时不丢数据。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PollerSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_interval_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant_interval_ticks: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant_poll_parallelism: Option<u64>,
}

/// 神经元容量与低价值回收配置（顶层 `neuron` 键）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NeuronSection {
    /// 活跃神经元数量上限；超过时后台定时回收最低价值节点。默认 300。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<usize>,
    /// 回收定时任务周期（毫秒）。默认 3_600_000（1h）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recycle_interval_ms: Option<u64>,
}

pub struct ConfigStore {
    storage_root: PathBuf,
}

impl ConfigStore {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }

    pub fn path(&self) -> PathBuf {
        self.storage_root.join("config.json")
    }

    pub fn read(&self) -> AppResult<AppConfigFile> {
        let path = self.path();
        if !path.exists() {
            return Ok(AppConfigFile::default());
        }
        let content = fs::read_to_string(&path).map_err(|e| {
            AppError::StorageError(format!("Failed to read {}: {e}", path.display()))
        })?;
        serde_json::from_str(&content)
            .map_err(|e| AppError::StorageError(format!("Invalid config.json: {e}")))
    }

    /// 读改写：`f` 中修改配置，随后整体写回（保留未建模字段）。
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
        let json = serde_json::to_string_pretty(&config).map_err(|e| {
            AppError::StorageError(format!("Failed to serialize config.json: {e}"))
        })?;
        fs::write(&path, json).map_err(|e| {
            AppError::StorageError(format!("Failed to write {}: {e}", path.display()))
        })
    }
}

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::error::{AppError, AppResult};
use super::models::{NeighborhoodPoolPolicy, SelectionPolicy, SessionBehavior, ToolPolicy};

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
    /// 内嵌网络服务配置（远程模式）。缺省 = 不启动，等价现状。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerSection>,
    /// 尚未建模的顶层字段原样保留，写回时不丢数据。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 内嵌 HTTP server 配置（顶层 `server` 键）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerSection {
    /// 是否启动内嵌 HTTP server；缺省 / false = 不启动。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// 监听地址；默认 127.0.0.1（仅本机）。跨机访问需显式改绑非 loopback。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// 监听端口；默认 8787。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// 远程访问白名单 token；空列表 = 本机免鉴权放行，非空 = 所有请求须携带列表内 token。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Vec<String>>,
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
    /// 会话规格默认值（顶层 `neuron.session_defaults`）；缺省回落硬编码默认。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_defaults: Option<SessionDefaultsSection>,
}

/// 会话规格默认值配置（`neuron.session_defaults`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionDefaultsSection {
    /// `session.assistant_dialogue` 的默认 behavior；缺省回落现状硬编码默认。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant_dialogue: Option<SessionBehavior>,
}

impl SessionDefaultsSection {
    /// 回落现状硬编码默认：Neighborhood 邻域选 + FromNeuron 工具 + 无契约段。
    pub fn fallback_assistant_dialogue() -> SessionBehavior {
        SessionBehavior {
            selection: SelectionPolicy::Neighborhood {
                policy: NeighborhoodPoolPolicy::default(),
            },
            tools: ToolPolicy::FromNeuron,
            insert_id: None,
        }
    }
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
        let json = serde_json::to_string_pretty(&config)
            .map_err(|e| AppError::StorageError(format!("Failed to serialize config.json: {e}")))?;
        fs::write(&path, json)
            .map_err(|e| AppError::StorageError(format!("Failed to write {}: {e}", path.display())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> AppConfigFile {
        serde_json::from_str(json).expect("valid config json")
    }

    #[test]
    fn server_section_defaults_to_none() {
        // 无 `server` 键 → 不启动内嵌服务（等价现状）。
        let config = parse(r#"{"poller":{"enabled":true}}"#);
        assert!(config.server.is_none());
    }

    #[test]
    fn server_section_parses_full_fields() {
        let config = parse(
            r#"{"server":{"enabled":true,"host":"0.0.0.0","port":9000,"tokens":["a","b"]}}"#,
        );
        let server = config.server.expect("server present");
        assert_eq!(server.enabled, Some(true));
        assert_eq!(server.host.as_deref(), Some("0.0.0.0"));
        assert_eq!(server.port, Some(9000));
        assert_eq!(server.tokens, Some(vec!["a".into(), "b".into()]));
    }

    #[test]
    fn server_section_absent_fields_are_none() {
        // 只给 enabled，其余缺省为 None（调用侧回落默认 host/port/tokens）。
        let config = parse(r#"{"server":{"enabled":true}}"#);
        let server = config.server.expect("server present");
        assert_eq!(server.enabled, Some(true));
        assert!(server.host.is_none());
        assert!(server.port.is_none());
        assert!(server.tokens.is_none());
    }

    #[test]
    fn server_section_roundtrips_with_extra_fields() {
        // 未建模顶层字段经 read/update 写回不丢失。
        let config = parse(r#"{"server":{"enabled":false},"future_key":42}"#);
        assert_eq!(config.extra.get("future_key").and_then(|v| v.as_i64()), Some(42));
        let json = serde_json::to_string(&config).expect("serialize");
        assert!(json.contains("future_key"));
        assert!(json.contains("server"));
    }
}

use std::{fs, path::PathBuf};

use serde::Deserialize;

use crate::core::{
    config::NeuronSection,
    error::AppResult,
};

/// 活跃神经元数量默认上限。
pub const DEFAULT_NEURON_CAPACITY: usize = 300;
/// 回收定时任务默认周期（1h）。
pub const DEFAULT_NEURON_RECYCLE_INTERVAL_MS: u64 = 3_600_000;

/// Fallback seed when config.json has no create_neuron_prompt.
pub const DEFAULT_CREATE_NEURON_PROMPT: &str = r#"You are the Neuron Creator for an agent app.
A neuron is a reusable capability node. Its `content` will later be used as system/knowledge text for selection and execution — write it to be executed, not marketed.

Return ONLY one JSON object (no markdown fences, no commentary):
{"desc":"string","content":"string","tool_ids":["string"]}

Field rules:
- desc: ≤20 chars Chinese/English label of a single responsibility (verb+noun preferred).
- content: a complete, self-contained prompt/knowledge block that includes:
  1) Role & goal
  2) When this neuron should be selected / when not
  3) Procedure or decision steps
  4) Output format / success criteria
  5) Hard constraints (what not to do)
  Prefer 200–800 Chinese characters (or equivalent). Avoid slogans, placeholders, and vague advice.
- tool_ids: only tools truly required for this role; otherwise []. Never invent tool names.
- weight: optional and ignored; the system always creates neurons with weight 0. Scores come only from later evaluation.

Quality bar:
- One neuron = one job. Do not merge unrelated responsibilities.
- Be concrete enough that another model can follow `content` without extra context.
- If the purpose is underspecified, make the safest useful specialist and state assumptions inside `content`.

Example (style only; do not copy blindly):
{"desc":"需求澄清","content":"你是需求澄清助手。当用户目标模糊时启用；目标已足够具体时可跳过。步骤：1) 用一句话复述目标 2) 列出缺口信息 3) 最多问 3 个关键问题 4) 给出可执行的下一版需求摘要。输出结构：目标/约束/待确认/下一步。禁止直接写实现代码或跳过澄清。","tool_ids":[]}"#;

#[derive(Debug, Clone)]
pub struct NeuronConfigReader {
    storage_root: PathBuf,
}

#[derive(Debug, Deserialize, Default)]
struct AppConfigSlice {
    neurons: Option<NeuronConfig>,
    /// 顶层 `neuron` 键：容量/回收配置。
    neuron: Option<NeuronSection>,
}

#[derive(Debug, Deserialize, Default)]
struct NeuronConfig {
    bootstrap: Option<NeuronBootstrapConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct NeuronBootstrapConfig {
    create_neuron_prompt: Option<String>,
}

impl NeuronConfigReader {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }

    /// Config override if present; otherwise built-in default seed.
    pub fn create_neuron_prompt(&self) -> AppResult<String> {
        let path = self.storage_root.join("config.json");
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let config: AppConfigSlice = serde_json::from_str(&content)?;
            if let Some(prompt) = config
                .neurons
                .and_then(|neurons| neurons.bootstrap)
                .and_then(|bootstrap| bootstrap.create_neuron_prompt)
                .filter(|prompt| !prompt.trim().is_empty())
            {
                return Ok(prompt);
            }
        }
        Ok(DEFAULT_CREATE_NEURON_PROMPT.to_string())
    }

    /// 活跃神经元容量上限；config.json 顶层 `neuron.capacity` 覆盖，默认 300。
    pub fn capacity(&self) -> AppResult<usize> {
        Ok(self
            .read_neuron_section()?
            .capacity
            .unwrap_or(DEFAULT_NEURON_CAPACITY))
    }

    /// 回收定时任务周期（毫秒）；config.json 顶层 `neuron.recycle_interval_ms` 覆盖，默认 1h。
    pub fn recycle_interval_ms(&self) -> AppResult<u64> {
        Ok(self
            .read_neuron_section()?
            .recycle_interval_ms
            .unwrap_or(DEFAULT_NEURON_RECYCLE_INTERVAL_MS))
    }

    fn read_neuron_section(&self) -> AppResult<NeuronSection> {
        let path = self.storage_root.join("config.json");
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let config: AppConfigSlice = serde_json::from_str(&content)?;
            return Ok(config.neuron.unwrap_or_default());
        }
        Ok(NeuronSection::default())
    }
}

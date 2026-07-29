use std::{fs, path::PathBuf};

use serde::Deserialize;

use super::error::AppResult;

/// Fallback seed when config.json has no create_neuron_prompt.
pub const DEFAULT_CREATE_NEURON_PROMPT: &str = r#"You create one neuron. Reply with ONLY a single JSON object, no markdown fences, no extra text.
Schema:
{"desc":"string","content":"string","weight":0.0,"tool_ids":["string"]}
Rules:
- desc: short label of the neuron's role
- content: the full prompt or knowledge text this neuron carries
- weight: finite number, typically 0 to 10
- tool_ids: allowed tool name list; use [] if none
- All string fields must be non-empty except tool_ids may be empty"#;

#[derive(Debug, Clone)]
pub struct NeuronConfigReader {
    storage_root: PathBuf,
}

#[derive(Debug, Deserialize, Default)]
struct AppConfigSlice {
    neurons: Option<NeuronConfig>,
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
}

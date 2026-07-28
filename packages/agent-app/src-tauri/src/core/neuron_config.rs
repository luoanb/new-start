use std::{fs, path::PathBuf};

use serde::Deserialize;

use super::error::{AppError, AppResult};

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

    pub fn create_neuron_prompt(&self) -> AppResult<String> {
        let path = self.storage_root.join("config.json");
        if !path.exists() {
            return Err(AppError::InvalidInput(
                "Missing neurons.bootstrap.create_neuron_prompt in .agent-app/config.json".into(),
            ));
        }
        let content = fs::read_to_string(path)?;
        let config: AppConfigSlice = serde_json::from_str(&content)?;
        config
            .neurons
            .and_then(|neurons| neurons.bootstrap)
            .and_then(|bootstrap| bootstrap.create_neuron_prompt)
            .filter(|prompt| !prompt.trim().is_empty())
            .ok_or_else(|| {
                AppError::InvalidInput(
                    "Missing neurons.bootstrap.create_neuron_prompt in .agent-app/config.json"
                        .into(),
                )
            })
    }
}

use super::{
    error::{AppError, AppResult},
    models::{
        ChatModelSelection, ModelCallRequest, ModelCallResponse, ModelCapabilities, ModelInfo,
        ModelMessage, ModelMessageRole, ProviderInfo, ProviderKind,
    },
};
use async_openai::{
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs,
    },
    Client,
};
use serde::Deserialize;
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Debug, Clone)]
pub struct ProviderRegistry {
    storage_root: PathBuf,
    providers: Vec<ProviderDefinition>,
}

#[derive(Debug, Clone)]
struct ProviderDefinition {
    info: ProviderInfo,
    api_base_env: &'static str,
    api_key_required: bool,
    allow_unlisted_models: bool,
}

#[derive(Debug, Deserialize, Default)]
struct AppConfig {
    defaults: Option<ConfigDefaults>,
    #[serde(default)]
    providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct ConfigDefaults {
    provider: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ProviderConfig {
    api_key: Option<String>,
    api_base: Option<String>,
    #[serde(default)]
    models: Vec<ConfiguredModel>,
}

#[derive(Debug, Deserialize)]
struct ConfiguredModel {
    id: String,
    display_name: Option<String>,
    #[serde(default)]
    capabilities: ModelCapabilities,
}

#[derive(Debug, Clone)]
struct ResolvedProviderConfig {
    api_key: String,
    api_base: Option<String>,
}

impl ProviderRegistry {
    pub fn new(storage_root: PathBuf) -> Self {
        Self {
            storage_root,
            providers: default_providers(),
        }
    }

    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .map(|provider| provider.info.clone())
            .collect()
    }

    pub fn list_models(&self, provider_id: Option<&str>) -> AppResult<Vec<ModelInfo>> {
        let file_config = self.read_config()?;
        if let Some(provider_id) = provider_id {
            let provider = self.require_provider(provider_id)?;
            Ok(configured_models(
                provider,
                file_config.providers.get(provider_id),
            ))
        } else {
            let mut models = Vec::new();
            for provider in &self.providers {
                models.extend(configured_models(
                    provider,
                    file_config.providers.get(&provider.info.id),
                ));
            }
            Ok(models)
        }
    }

    pub fn default_model_selection(&self) -> AppResult<Option<ChatModelSelection>> {
        let file_config = self.read_config()?;
        let Some(defaults) = file_config.defaults else {
            return Ok(None);
        };

        match (defaults.provider, defaults.model) {
            (Some(provider_id), Some(model_id)) => {
                self.require_model(&provider_id, &model_id)?;
                Ok(Some(ChatModelSelection {
                    provider_id,
                    model_id,
                }))
            }
            (None, None) => Ok(None),
            _ => Err(AppError::InvalidInput(
                "`defaults.provider` and `defaults.model` must be configured together".into(),
            )),
        }
    }

    pub fn require_model(&self, provider_id: &str, model_id: &str) -> AppResult<()> {
        let provider = self.require_provider(provider_id)?;
        let file_config = self.read_config()?;
        self.require_model_definition(provider, file_config.providers.get(provider_id), model_id)
    }

    pub async fn call_model(&self, request: ModelCallRequest) -> AppResult<ModelCallResponse> {
        self.validate_request(&request)?;

        self.require_model(&request.provider_id, &request.model_id)?;
        let provider = self.require_provider(&request.provider_id)?;
        let config = self.resolve_provider_config(provider)?;
        let client = Client::with_config(build_openai_config(config));
        let messages = request
            .messages
            .iter()
            .map(to_chat_message)
            .collect::<AppResult<Vec<_>>>()?;

        let chat_request = CreateChatCompletionRequestArgs::default()
            .model(&request.model_id)
            .messages(messages)
            .build()
            .map_err(|error| AppError::InvalidInput(error.to_string()))?;

        let response = client
            .chat()
            .create(chat_request)
            .await
            .map_err(|error| AppError::LlmRequestFailed(error.to_string()))?;

        let output = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .ok_or_else(|| AppError::LlmRequestFailed("Provider returned no text output".into()))?;

        Ok(ModelCallResponse {
            provider_id: request.provider_id,
            model_id: request.model_id,
            output,
        })
    }

    fn validate_request(&self, request: &ModelCallRequest) -> AppResult<()> {
        if request.provider_id.trim().is_empty() {
            return Err(AppError::InvalidInput("Provider id cannot be empty".into()));
        }

        if request.model_id.trim().is_empty() {
            return Err(AppError::InvalidInput("Model id cannot be empty".into()));
        }

        if request.messages.is_empty() {
            return Err(AppError::InvalidInput("Messages cannot be empty".into()));
        }

        for message in &request.messages {
            if message.content.trim().is_empty() {
                return Err(AppError::InvalidInput(
                    "Model message content cannot be empty".into(),
                ));
            }
        }

        Ok(())
    }

    fn require_provider(&self, provider_id: &str) -> AppResult<&ProviderDefinition> {
        self.providers
            .iter()
            .find(|provider| provider.info.id == provider_id)
            .ok_or_else(|| AppError::ProviderNotFound(provider_id.to_string()))
    }

    fn require_model_definition(
        &self,
        provider: &ProviderDefinition,
        provider_config: Option<&ProviderConfig>,
        model_id: &str,
    ) -> AppResult<()> {
        if provider.allow_unlisted_models {
            return Ok(());
        }

        if configured_models(provider, provider_config)
            .iter()
            .any(|model| model.id == model_id)
        {
            Ok(())
        } else {
            Err(AppError::ModelNotFound(model_id.to_string()))
        }
    }

    fn resolve_provider_config(
        &self,
        provider: &ProviderDefinition,
    ) -> AppResult<ResolvedProviderConfig> {
        let file_config = self.read_config()?;
        let provider_config = file_config.providers.get(&provider.info.id);
        let api_key = std::env::var(&provider.info.auth_env)
            .ok()
            .or_else(|| provider_config.and_then(|config| config.api_key.clone()));

        let api_key = match api_key {
            Some(value) if !value.trim().is_empty() => value,
            _ if provider.api_key_required => {
                return Err(AppError::ProviderAuthMissing(format!(
                    "Missing API key for provider `{}`. Set {} or .agent-app/config.json.",
                    provider.info.id, provider.info.auth_env
                )));
            }
            _ => "local-provider".to_string(),
        };

        let api_base = std::env::var(provider.api_base_env)
            .ok()
            .or_else(|| provider_config.and_then(|config| config.api_base.clone()))
            .or_else(|| provider.info.api_base.clone());

        Ok(ResolvedProviderConfig { api_key, api_base })
    }

    fn read_config(&self) -> AppResult<AppConfig> {
        let path = self.storage_root.join("config.json");
        if !path.exists() {
            return Ok(AppConfig::default());
        }

        let content = fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(Into::into)
    }
}

fn to_chat_message(message: &ModelMessage) -> AppResult<ChatCompletionRequestMessage> {
    match message.role {
        ModelMessageRole::System => ChatCompletionRequestSystemMessageArgs::default()
            .content(message.content.clone())
            .build()
            .map(Into::into)
            .map_err(|error| AppError::InvalidInput(error.to_string())),
        ModelMessageRole::User => ChatCompletionRequestUserMessageArgs::default()
            .content(message.content.clone())
            .build()
            .map(Into::into)
            .map_err(|error| AppError::InvalidInput(error.to_string())),
        ModelMessageRole::Assistant => ChatCompletionRequestAssistantMessageArgs::default()
            .content(message.content.clone())
            .build()
            .map(Into::into)
            .map_err(|error| AppError::InvalidInput(error.to_string())),
    }
}

fn build_openai_config(config: ResolvedProviderConfig) -> OpenAIConfig {
    let mut openai_config = OpenAIConfig::new().with_api_key(config.api_key);
    if let Some(api_base) = config.api_base {
        openai_config = openai_config.with_api_base(api_base);
    }
    openai_config
}

fn default_providers() -> Vec<ProviderDefinition> {
    vec![
        provider(
            "openai",
            "OpenAI",
            None,
            "OPENAI_API_KEY",
            "OPENAI_BASE_URL",
            true,
            false,
            ProviderKind::OpenAi,
        ),
        provider(
            "deepseek",
            "DeepSeek",
            Some("https://api.deepseek.com/v1"),
            "DEEPSEEK_API_KEY",
            "DEEPSEEK_BASE_URL",
            true,
            false,
            ProviderKind::OpenAiCompatible,
        ),
        provider(
            "ollama",
            "Ollama OpenAI-compatible",
            Some("http://localhost:11434/v1"),
            "OLLAMA_API_KEY",
            "OLLAMA_BASE_URL",
            false,
            true,
            ProviderKind::OpenAiCompatible,
        ),
        provider(
            "custom",
            "Custom OpenAI-compatible",
            None,
            "CUSTOM_OPENAI_API_KEY",
            "CUSTOM_OPENAI_BASE_URL",
            true,
            true,
            ProviderKind::OpenAiCompatible,
        ),
    ]
}

fn provider(
    id: &str,
    display_name: &str,
    api_base: Option<&str>,
    auth_env: &'static str,
    api_base_env: &'static str,
    api_key_required: bool,
    allow_unlisted_models: bool,
    kind: ProviderKind,
) -> ProviderDefinition {
    ProviderDefinition {
        info: ProviderInfo {
            id: id.to_string(),
            display_name: display_name.to_string(),
            api_base: api_base.map(str::to_string),
            auth_env: auth_env.to_string(),
            kind,
        },
        api_base_env,
        api_key_required,
        allow_unlisted_models,
    }
}

fn configured_models(
    provider: &ProviderDefinition,
    provider_config: Option<&ProviderConfig>,
) -> Vec<ModelInfo> {
    provider_config
        .map(|config| {
            config
                .models
                .iter()
                .filter(|model| !model.id.trim().is_empty())
                .map(|model| ModelInfo {
                    id: model.id.clone(),
                    provider_id: provider.info.id.clone(),
                    display_name: model
                        .display_name
                        .clone()
                        .unwrap_or_else(|| model.id.clone()),
                    capabilities: model.capabilities.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn list_models_filters_by_provider() {
        let root = test_root("list_models_filters_by_provider");
        write_config(
            &root,
            r#"{
              "providers": {
                "deepseek": {
                  "models": [
                    {
                      "id": "deepseek-v4-flash",
                      "display_name": "DeepSeek V4 Flash"
                    }
                  ]
                }
              }
            }"#,
        );
        let registry = ProviderRegistry::new(root);

        let models = registry
            .list_models(Some("deepseek"))
            .expect("provider should exist");

        assert!(models.iter().all(|model| model.provider_id == "deepseek"));
        assert!(models.iter().any(|model| model.id == "deepseek-v4-flash"));
    }

    #[test]
    fn default_model_selection_reads_config() {
        let root = test_root("default_model_selection_reads_config");
        write_config(
            &root,
            r#"{
              "defaults": {
                "provider": "deepseek",
                "model": "deepseek-v4-flash"
              },
              "providers": {
                "deepseek": {
                  "models": [
                    {
                      "id": "deepseek-v4-flash",
                      "display_name": "DeepSeek V4 Flash"
                    }
                  ]
                }
              }
            }"#,
        );
        let registry = ProviderRegistry::new(root);

        let selection = registry
            .default_model_selection()
            .expect("defaults should load")
            .expect("defaults should be configured");

        assert_eq!(selection.provider_id, "deepseek");
        assert_eq!(selection.model_id, "deepseek-v4-flash");
    }

    #[test]
    fn unknown_provider_returns_error() {
        let registry = ProviderRegistry::new(std::env::temp_dir());

        let error = registry
            .list_models(Some("missing"))
            .expect_err("provider should be rejected");

        assert_eq!(error.code(), "provider_not_found");
    }

    #[tokio::test]
    async fn call_model_rejects_unknown_model_before_network() {
        let registry = ProviderRegistry::new(std::env::temp_dir());
        let request = ModelCallRequest {
            provider_id: "openai".to_string(),
            model_id: "missing-model".to_string(),
            messages: vec![ModelMessage {
                role: ModelMessageRole::User,
                content: "hello".to_string(),
            }],
        };

        let error = registry
            .call_model(request)
            .await
            .expect_err("unknown model should be rejected");

        assert_eq!(error.code(), "model_not_found");
    }

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-app-providers-{name}-{}",
            crate::core::storage::now_ms()
        ))
    }

    fn write_config(root: &PathBuf, config: &str) {
        fs::create_dir_all(root).expect("test config root should be created");
        fs::write(root.join("config.json"), config).expect("test config should be written");
    }
}

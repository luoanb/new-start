use super::{
    error::{AppError, AppResult},
    models::{
        ModelCallRequest, ModelCallResponse, ModelCapabilities, ModelInfo, ModelMessage,
        ModelMessageRole, ProviderInfo, ProviderKind,
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
    models: Vec<ModelInfo>,
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
    #[serde(default)]
    providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct ProviderConfig {
    api_key: Option<String>,
    api_base: Option<String>,
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
            models: default_models(),
        }
    }

    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .map(|provider| provider.info.clone())
            .collect()
    }

    pub fn list_models(&self, provider_id: Option<&str>) -> AppResult<Vec<ModelInfo>> {
        if let Some(provider_id) = provider_id {
            self.require_provider(provider_id)?;
            Ok(self
                .models
                .iter()
                .filter(|model| model.provider_id == provider_id)
                .cloned()
                .collect())
        } else {
            Ok(self.models.clone())
        }
    }

    pub async fn call_model(&self, request: ModelCallRequest) -> AppResult<ModelCallResponse> {
        self.validate_request(&request)?;

        let provider = self.require_provider(&request.provider_id)?;
        self.require_model(provider, &request.model_id)?;
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

    fn require_model(&self, provider: &ProviderDefinition, model_id: &str) -> AppResult<()> {
        if provider.allow_unlisted_models {
            return Ok(());
        }

        if self
            .models
            .iter()
            .any(|model| model.provider_id == provider.info.id && model.id == model_id)
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

fn default_models() -> Vec<ModelInfo> {
    vec![
        model("openai", "gpt-4o-mini", "GPT-4o mini", true, true),
        model("openai", "gpt-4.1-mini", "GPT-4.1 mini", true, true),
        model("deepseek", "deepseek-chat", "DeepSeek Chat", true, false),
        model(
            "deepseek",
            "deepseek-reasoner",
            "DeepSeek Reasoner",
            true,
            false,
        ),
        model("ollama", "llama3.1", "Llama 3.1", true, false),
        model("ollama", "qwen2.5", "Qwen 2.5", true, false),
    ]
}

fn model(
    provider_id: &str,
    id: &str,
    display_name: &str,
    tools: bool,
    streaming: bool,
) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        provider_id: provider_id.to_string(),
        display_name: display_name.to_string(),
        capabilities: ModelCapabilities {
            chat: true,
            tools,
            streaming,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_models_filters_by_provider() {
        let registry = ProviderRegistry::new(std::env::temp_dir());

        let models = registry
            .list_models(Some("deepseek"))
            .expect("provider should exist");

        assert!(models.iter().all(|model| model.provider_id == "deepseek"));
        assert!(models.iter().any(|model| model.id == "deepseek-chat"));
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
}

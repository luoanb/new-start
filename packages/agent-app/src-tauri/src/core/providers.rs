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
        ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls, ChatCompletionRequestAssistantMessageArgs,
        ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs,
        ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessageArgs,
        ChatCompletionTool, ChatCompletionTools, CreateChatCompletionRequestArgs, FinishReason,
        FunctionCall, FunctionObject,
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
    context_window: Option<u32>,
    max_output_tokens: Option<u32>,
    pricing_input: Option<f64>,
    pricing_output: Option<f64>,
    pricing_cache_input: Option<f64>,
    knowledge_cutoff: Option<String>,
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

        let mut args = CreateChatCompletionRequestArgs::default();
        let builder = args.model(&request.model_id).messages(messages);

        // Attach tools if present
        if let Some(tools) = &request.tools {
            let openai_tools: Vec<ChatCompletionTools> = tools
                .iter()
                .map(|t| {
                    ChatCompletionTools::Function(ChatCompletionTool {
                        function: FunctionObject {
                            name: t.name.clone(),
                            description: Some(t.description.clone()),
                            parameters: Some(t.parameters.clone()),
                            strict: None,
                        },
                    })
                })
                .collect();
            builder.tools(openai_tools);
        }

        let chat_request = args
            .build()
            .map_err(|error| AppError::InvalidInput(error.to_string()))?;

        let response = client
            .chat()
            .create(chat_request)
            .await
            .map_err(|error| AppError::LlmRequestFailed(error.to_string()))?;

        let choice = response.choices.first().ok_or_else(|| {
            AppError::LlmRequestFailed("Provider returned no choices".into())
        })?;

        // Parse finish_reason
        let finish_reason = match choice.finish_reason {
            Some(FinishReason::Stop) => "stop",
            Some(FinishReason::ToolCalls) => "tool_calls",
            Some(FinishReason::Length) => "length",
            Some(FinishReason::ContentFilter) => "content_filter",
            Some(FinishReason::FunctionCall) => "function_call",
            None => "stop",
        }
        .to_string();

        // Parse tool_calls from response
        let tool_calls: Option<Vec<super::models::ToolCall>> = choice
            .message
            .tool_calls
            .as_ref()
            .map(|calls| {
                calls
                    .iter()
                    .filter_map(|tc| match tc {
                        ChatCompletionMessageToolCalls::Function(ftc) => {
                            let name = ftc.function.name.clone();
                            let args: serde_json::Value =
                                serde_json::from_str(&ftc.function.arguments).unwrap_or_default();
                            Some(super::models::ToolCall {
                                id: ftc.id.clone(),
                                name,
                                arguments: args,
                            })
                        }
                        _ => None,
                    })
                    .collect()
            })
            .filter(|v: &Vec<_>| !v.is_empty());

        // Extract text output (may be None when tool_calls is present)
        let output = choice.message.content.clone().unwrap_or_default();

        Ok(ModelCallResponse {
            provider_id: request.provider_id,
            model_id: request.model_id,
            output,
            tool_calls,
            finish_reason,
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
        ModelMessageRole::Assistant => {
            let mut args = ChatCompletionRequestAssistantMessageArgs::default();
            if !message.content.is_empty() {
                args.content(
                    ChatCompletionRequestAssistantMessageContent::Text(message.content.clone()),
                );
            }
            if let Some(tc) = &message.tool_calls {
                let tool_calls: Vec<ChatCompletionMessageToolCalls> = tc
                    .iter()
                    .map(|t| {
                        ChatCompletionMessageToolCalls::Function(ChatCompletionMessageToolCall {
                            id: t.id.clone(),
                            function: FunctionCall {
                                name: t.name.clone(),
                                arguments: t.arguments.to_string(),
                            },
                        })
                    })
                    .collect();
                args.tool_calls(tool_calls);
            }
            args.build()
                .map(Into::into)
                .map_err(|error| AppError::InvalidInput(error.to_string()))
        }
        ModelMessageRole::Tool => {
            let tool_call_id = message.tool_call_id.clone().ok_or_else(|| {
                AppError::InvalidInput("Tool message missing tool_call_id".into())
            })?;
            ChatCompletionRequestToolMessageArgs::default()
                .tool_call_id(tool_call_id)
                .content(
                    ChatCompletionRequestToolMessageContent::Text(message.content.clone()),
                )
                .build()
                .map(Into::into)
                .map_err(|error| AppError::InvalidInput(error.to_string()))
        }
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
                    context_window: model.context_window,
                    max_output_tokens: model.max_output_tokens,
                    pricing_input: model.pricing_input,
                    pricing_output: model.pricing_output,
                    pricing_cache_input: model.pricing_cache_input,
                    knowledge_cutoff: model.knowledge_cutoff.clone(),
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
                tool_calls: None,
                tool_call_id: None,
            }],
            tools: None,
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
            crate::core::conversation_store::now_ms()
        ))
    }

    fn write_config(root: &PathBuf, config: &str) {
        fs::create_dir_all(root).expect("test config root should be created");
        fs::write(root.join("config.json"), config).expect("test config should be written");
    }
}

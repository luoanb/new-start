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
        ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestAssistantMessageContent,
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestToolMessageArgs, ChatCompletionRequestToolMessageContent,
        ChatCompletionRequestUserMessageArgs, ChatCompletionTool, ChatCompletionTools,
        CreateChatCompletionRequestArgs, FinishReason, FunctionCall, FunctionObject,
    },
    Client,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{Arc, RwLock},
};

/// 前端编辑器回显的 API Key 掩码；`save_config` 收到与掩码相同的值时视为「未修改」。
const MASKED_API_KEY: &str = "sk-****";

#[derive(Debug, Clone)]
pub struct ProviderRegistry {
    storage_root: PathBuf,
    providers: Arc<RwLock<Vec<ProviderDefinition>>>,
}

#[derive(Debug, Clone)]
struct ProviderDefinition {
    info: ProviderInfo,
    api_base_env: Option<String>,
    api_key_required: bool,
    allow_unlisted_models: bool,
    /// 内置服务商（定义在代码中）标记，用于编辑视图与删除语义。
    builtin: bool,
    /// false = 禁用隐藏（内置删除 = 写 enabled:false，代码定义不物理删除）。
    enabled: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<ProviderKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(default)]
    models: Vec<ConfiguredModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfiguredModel {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(default)]
    capabilities: ModelCapabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context_window: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pricing_input: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pricing_output: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pricing_cache_input: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    knowledge_cutoff: Option<String>,
}

/// 默认模型（编辑器可整体管理；与 `defaults` 对应）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ProviderDefaults {
    pub provider: String,
    pub model: String,
}

/// 单个服务商的可编辑视图（掩码回显 api_key，携带内置/启用元信息）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEditInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default = "default_provider_kind")]
    pub kind: ProviderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
    /// 掩码回显；写回时与 `MASKED_API_KEY` 相同视为未修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// 是否已配置 API Key（env 或 config），仅回显用。
    #[serde(default)]
    pub api_key_set: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_env: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub models: Vec<ModelEditInfo>,
}

/// 单个模型的可编辑视图（与 `ConfiguredModel` 对应）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEditInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_input: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_output: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_cache_input: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_cutoff: Option<String>,
}

/// 服务商+默认模型的完整可编辑配置视图（前端 main 区编辑器整体读写）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfigView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defaults: Option<ProviderDefaults>,
    #[serde(default)]
    pub providers: Vec<ProviderEditInfo>,
}

fn default_true() -> bool {
    true
}

fn default_provider_kind() -> ProviderKind {
    ProviderKind::OpenAiCompatible
}

impl ProviderRegistry {
    pub fn new(storage_root: PathBuf) -> Self {
        let providers = Arc::new(RwLock::new(Vec::new()));
        let registry = Self {
            storage_root,
            providers,
        };
        // 装配期失败仅记录（读配置失败回落内置全集），与旧行为一致：配置非法不阻断启动。
        if let Err(error) = registry.reload() {
            tracing::warn!(
                error = %error,
                "provider registry assemble failed, falling back to defaults"
            );
        }
        registry
    }

    /// 热重载：重读 config.json 并重新装配 providers（保存即生效，无需重启）。
    pub fn reload(&self) -> AppResult<()> {
        let file_config = self.read_config()?;
        let assembled = self.assemble(&file_config);
        let mut guard = self
            .providers
            .write()
            .map_err(|e| AppError::RuntimeError(format!("providers lock: {e}")))?;
        *guard = assembled;
        Ok(())
    }

    /// 装配运行期 providers：内置（过滤 config 中 enabled=false 的）+ config 自定义。
    fn assemble(&self, file_config: &AppConfig) -> Vec<ProviderDefinition> {
        let mut out: Vec<ProviderDefinition> = Vec::new();
        let builtins = default_providers();
        let builtin_ids: HashSet<String> =
            builtins.iter().map(|p| p.info.id.clone()).collect();

        for mut p in builtins {
            let cfg = file_config.providers.get(&p.info.id);
            // 内置禁用标记：enabled == Some(false) → 从运行集隐藏。
            if cfg.and_then(|c| c.enabled) == Some(false) {
                continue;
            }
            p.builtin = true;
            p.enabled = true;
            out.push(p);
        }
        for (id, cfg) in &file_config.providers {
            if builtin_ids.contains(id) {
                continue;
            }
            // 自定义服务商：enabled=false 表示已删除，不装配。
            if cfg.enabled == Some(false) {
                continue;
            }
            out.push(custom_definition(id, cfg));
        }
        out
    }

    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .read()
            .map(|guard| guard.iter().map(|provider| provider.info.clone()).collect())
            .unwrap_or_default()
    }

    pub fn list_models(&self, provider_id: Option<&str>) -> AppResult<Vec<ModelInfo>> {
        let file_config = self.read_config()?;
        let providers = self
            .providers
            .read()
            .map_err(|e| AppError::RuntimeError(format!("providers lock: {e}")))?;
        if let Some(provider_id) = provider_id {
            let provider = providers
                .iter()
                .find(|p| p.info.id == provider_id)
                .ok_or_else(|| AppError::ProviderNotFound(provider_id.to_string()))?;
            Ok(configured_models(
                provider,
                file_config.providers.get(provider_id),
            ))
        } else {
            let mut models = Vec::new();
            for provider in providers.iter() {
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
        self.require_model_definition(&provider, file_config.providers.get(provider_id), model_id)
    }

    pub async fn call_model(&self, request: ModelCallRequest) -> AppResult<ModelCallResponse> {
        self.validate_request(&request)?;

        self.require_model(&request.provider_id, &request.model_id)?;
        let provider = self.require_provider(&request.provider_id)?;
        let config = self.resolve_provider_config(&provider)?;
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

        let choice = response
            .choices
            .first()
            .ok_or_else(|| AppError::LlmRequestFailed("Provider returned no choices".into()))?;

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

        // 空响应防御：模型返回 HTTP 200 但无文本且无 tool_calls 时视为异常，
        // 避免空消息落库后污染历史（providers 校验会拒绝空 assistant 消息锁死会话）。
        if output.trim().is_empty() && tool_calls.is_none() {
            return Err(AppError::LlmRequestFailed(
                "Provider returned empty response without tool_calls".into(),
            ));
        }

        Ok(ModelCallResponse {
            provider_id: request.provider_id,
            model_id: request.model_id,
            output,
            tool_calls,
            finish_reason,
        })
    }

    // ── 管理面（main 区编辑器）──

    /// 返回可编辑完整视图：内置全量列出（含禁用的，便于重新启用）+ config 自定义；
    /// api_key 一律掩码回显。
    pub fn get_config_view(&self) -> AppResult<ProviderConfigView> {
        let file_config = self.read_config()?;
        let mut providers = Vec::new();
        let builtins = default_providers();

        for p in &builtins {
            let cfg = file_config.providers.get(&p.info.id);
            let enabled = cfg.and_then(|c| c.enabled).unwrap_or(true);
            let (api_key_set, api_key) = self.api_key_state(&p.info.auth_env, cfg);
            providers.push(ProviderEditInfo {
                id: p.info.id.clone(),
                display_name: cfg
                    .and_then(|c| c.display_name.clone())
                    .or_else(|| Some(p.info.display_name.clone())),
                kind: p.info.kind.clone(),
                api_base: cfg
                    .and_then(|c| c.api_base.clone())
                    .or_else(|| p.info.api_base.clone()),
                api_key,
                api_key_set,
                auth_env: Some(p.info.auth_env.clone()),
                enabled,
                builtin: true,
                models: configured_models_edit(cfg),
            });
        }
        for (id, cfg) in &file_config.providers {
            if builtins.iter().any(|p| p.info.id == *id) {
                continue;
            }
            let (api_key_set, api_key) =
                self.api_key_state(cfg.auth_env.as_deref().unwrap_or(""), Some(cfg));
            providers.push(ProviderEditInfo {
                id: id.clone(),
                display_name: cfg.display_name.clone(),
                kind: cfg.kind.clone().unwrap_or(ProviderKind::OpenAiCompatible),
                api_base: cfg.api_base.clone(),
                api_key,
                api_key_set,
                auth_env: cfg.auth_env.clone(),
                enabled: cfg.enabled.unwrap_or(true),
                builtin: false,
                models: configured_models_edit(Some(cfg)),
            });
        }

        let defaults = file_config.defaults.map(|d| ProviderDefaults {
            provider: d.provider.clone().unwrap_or_default(),
            model: d.model.clone().unwrap_or_default(),
        });

        Ok(ProviderConfigView { defaults, providers })
    }

    /// 保存服务商配置：校验 → 合并写回 config.json（保留 defaults/poller 等其它顶层键）→ 热重载。
    pub fn save_config(&self, view: ProviderConfigView) -> AppResult<ProviderConfigView> {
        validate_provider_config(&view)?;
        self.write_config(&view)?;
        self.reload()?;
        self.get_config_view()
    }

    /// 掩码回显 api_key：env 或 config 任一有值 → (true, 掩码)。
    fn api_key_state(&self, auth_env: &str, cfg: Option<&ProviderConfig>) -> (bool, Option<String>) {
        let from_env = if auth_env.trim().is_empty() {
            None
        } else {
            std::env::var(auth_env)
                .ok()
                .filter(|v| !v.trim().is_empty())
        };
        let from_cfg = cfg
            .and_then(|c| c.api_key.clone())
            .filter(|v| !v.trim().is_empty());
        let set = from_env.is_some() || from_cfg.is_some();
        (
            set,
            if set {
                Some(MASKED_API_KEY.to_string())
            } else {
                None
            },
        )
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
            // OpenAI 兼容规范：assistant 的 tool_calls 消息可以没有正文（content 为空是合法形态）。
            // 仅对无 tool_calls 的消息强制非空，避免合法工具调用历史被本地校验误拒。
            let is_tool_call = message.role == ModelMessageRole::Assistant
                && message
                    .tool_calls
                    .as_ref()
                    .is_some_and(|calls| !calls.is_empty());
            if !is_tool_call && message.content.trim().is_empty() {
                return Err(AppError::InvalidInput(
                    "Model message content cannot be empty".into(),
                ));
            }
        }

        Ok(())
    }

    fn require_provider(&self, provider_id: &str) -> AppResult<ProviderDefinition> {
        self.providers
            .read()
            .map_err(|e| AppError::RuntimeError(format!("providers lock: {e}")))?
            .iter()
            .find(|provider| provider.info.id == provider_id)
            .cloned()
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

        let api_key = if provider.info.auth_env.trim().is_empty() {
            None
        } else {
            std::env::var(&provider.info.auth_env).ok()
        }
        .or_else(|| provider_config.and_then(|config| config.api_key.clone()));

        let api_key = match api_key {
            Some(value) if !value.trim().is_empty() => value,
            _ if provider.api_key_required => {
                return Err(AppError::ProviderAuthMissing(format!(
                    "Missing API key for provider `{}`. Set {} or .pulsar/config.json.",
                    provider.info.id, provider.info.auth_env
                )));
            }
            _ => "local-provider".to_string(),
        };

        let api_base = match &provider.api_base_env {
            Some(name) if !name.trim().is_empty() => std::env::var(name).ok(),
            _ => None,
        }
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

    /// 原子写回 config.json：只更新 `defaults` / `providers` 两个键，其余顶层键（poller 等）原样保留。
    fn write_config(&self, view: &ProviderConfigView) -> AppResult<()> {
        let path = self.storage_root.join("config.json");
        let mut root: serde_json::Value = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if let Some(defaults) = &view.defaults {
            root["defaults"] = serde_json::json!({
                "provider": defaults.provider,
                "model": defaults.model,
            });
        } else {
            root.as_object_mut().map(|obj| obj.remove("defaults"));
        }

        root["providers"] = build_providers_json(view, &root);

        let content = serde_json::to_string_pretty(&root)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, content)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }
}

/// 由编辑视图构建写回的 providers JSON 映射。
///
/// - api_key：掩码/空 → 保留原 config 值；新值 → 覆盖写回。
/// - 内置：显式写 `enabled`（false = 禁用，代码定义不物理删除）；
/// - 自定义：`enabled: false` → 从映射移除（删除语义）。
fn build_providers_json(view: &ProviderConfigView, root: &serde_json::Value) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for p in &view.providers {
        let mut obj = serde_json::Map::new();

        if let Some(dn) = &p.display_name {
            obj.insert("display_name".into(), serde_json::json!(dn));
        }
        obj.insert("kind".into(), serde_json::json!(p.kind));
        if let Some(base) = &p.api_base {
            obj.insert("api_base".into(), serde_json::json!(base));
        }
        if let Some(env) = &p.auth_env {
            if !env.trim().is_empty() {
                obj.insert("auth_env".into(), serde_json::json!(env));
            }
        }

        let has_new_key = p
            .api_key
            .as_deref()
            .is_some_and(|k| !k.is_empty() && k != MASKED_API_KEY);
        if has_new_key {
            obj.insert("api_key".into(), serde_json::json!(p.api_key));
        } else if let Some(existing) = root
            .get("providers")
            .and_then(|v| v.get(&p.id))
            .and_then(|v| v.get("api_key"))
            .and_then(|v| v.as_str())
        {
            // 掩码/空：保留原配置中的 api_key。
            obj.insert("api_key".into(), serde_json::json!(existing));
        }

        let models: Vec<serde_json::Value> = p
            .models
            .iter()
            .filter(|m| !m.id.trim().is_empty())
            .map(|m| {
                let mut mo = serde_json::Map::new();
                mo.insert("id".into(), serde_json::json!(m.id));
                if let Some(dn) = &m.display_name {
                    mo.insert("display_name".into(), serde_json::json!(dn));
                }
                mo.insert(
                    "capabilities".into(),
                    serde_json::to_value(&m.capabilities).unwrap_or_default(),
                );
                if let Some(v) = m.context_window {
                    mo.insert("context_window".into(), serde_json::json!(v));
                }
                if let Some(v) = m.max_output_tokens {
                    mo.insert("max_output_tokens".into(), serde_json::json!(v));
                }
                if let Some(v) = m.pricing_input {
                    mo.insert("pricing_input".into(), serde_json::json!(v));
                }
                if let Some(v) = m.pricing_output {
                    mo.insert("pricing_output".into(), serde_json::json!(v));
                }
                if let Some(v) = m.pricing_cache_input {
                    mo.insert("pricing_cache_input".into(), serde_json::json!(v));
                }
                if let Some(v) = &m.knowledge_cutoff {
                    mo.insert("knowledge_cutoff".into(), serde_json::json!(v));
                }
                serde_json::Value::Object(mo)
            })
            .collect();

        if p.builtin {
            obj.insert("enabled".into(), serde_json::json!(p.enabled));
            obj.insert("models".into(), serde_json::Value::Array(models));
            map.insert(p.id.clone(), serde_json::Value::Object(obj));
        } else if p.enabled {
            // 自定义：enabled=false = 删除；true = 正常写回（不写 enabled 字段，默认启用）。
            obj.insert("models".into(), serde_json::Value::Array(models));
            map.insert(p.id.clone(), serde_json::Value::Object(obj));
        }
    }
    serde_json::Value::Object(map)
}

/// 保存前校验：id 合法性/冲突、模型 id 非空、defaults 引用存在。
fn validate_provider_config(view: &ProviderConfigView) -> AppResult<()> {
    let mut ids = HashSet::new();
    for p in &view.providers {
        let id = p.id.trim();
        if id.is_empty() {
            return Err(AppError::InvalidInput(
                "Provider id cannot be empty".into(),
            ));
        }
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(AppError::InvalidInput(format!(
                "Provider id `{id}` contains invalid characters (allowed: a-z, 0-9, _, -)"
            )));
        }
        if !ids.insert(id.to_string()) {
            return Err(AppError::InvalidInput(format!(
                "Duplicate provider id: `{id}`"
            )));
        }
        for m in &p.models {
            if m.id.trim().is_empty() {
                return Err(AppError::InvalidInput(format!(
                    "Provider `{id}` has a model with empty id"
                )));
            }
        }
    }

    if let Some(defaults) = &view.defaults {
        if !defaults.provider.is_empty() || !defaults.model.is_empty() {
            let provider = view
                .providers
                .iter()
                .find(|p| p.id == defaults.provider && p.enabled)
                .ok_or_else(|| {
                    AppError::InvalidInput(format!(
                        "Default provider `{}` is missing or disabled",
                        defaults.provider
                    ))
                })?;
            if !provider.models.iter().any(|m| m.id == defaults.model) {
                return Err(AppError::InvalidInput(format!(
                    "Default model `{}` is not configured under provider `{}`",
                    defaults.model, defaults.provider
                )));
            }
        }
    }
    Ok(())
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
                args.content(ChatCompletionRequestAssistantMessageContent::Text(
                    message.content.clone(),
                ));
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
                .content(ChatCompletionRequestToolMessageContent::Text(
                    message.content.clone(),
                ))
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

#[derive(Debug, Clone)]
struct ResolvedProviderConfig {
    api_key: String,
    api_base: Option<String>,
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
        api_base_env: Some(api_base_env.to_string()),
        api_key_required,
        allow_unlisted_models,
        builtin: true,
        enabled: true,
    }
}

/// 由 config.json 构造自定义服务商定义。
fn custom_definition(id: &str, cfg: &ProviderConfig) -> ProviderDefinition {
    ProviderDefinition {
        info: ProviderInfo {
            id: id.to_string(),
            display_name: cfg
                .display_name
                .clone()
                .unwrap_or_else(|| id.to_string()),
            api_base: cfg.api_base.clone(),
            auth_env: cfg.auth_env.clone().unwrap_or_default(),
            kind: cfg.kind.clone().unwrap_or(ProviderKind::OpenAiCompatible),
        },
        api_base_env: None,
        api_key_required: true,
        allow_unlisted_models: true,
        builtin: false,
        enabled: cfg.enabled.unwrap_or(true),
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

/// 编辑视图的模型列表（完整字段）。
fn configured_models_edit(provider_config: Option<&ProviderConfig>) -> Vec<ModelEditInfo> {
    provider_config
        .map(|config| {
            config
                .models
                .iter()
                .map(|model| ModelEditInfo {
                    id: model.id.clone(),
                    display_name: model.display_name.clone(),
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

    // ── 管理面测试 ──

    #[test]
    fn disabled_builtin_is_filtered_from_list() {
        let root = test_root("disabled_builtin_is_filtered_from_list");
        write_config(
            &root,
            r#"{
              "providers": {
                "ollama": { "enabled": false }
              }
            }"#,
        );
        let registry = ProviderRegistry::new(root);

        let providers = registry.list_providers();
        assert!(providers.iter().any(|p| p.id == "openai"));
        assert!(!providers.iter().any(|p| p.id == "ollama"));
    }

    #[test]
    fn custom_provider_roundtrip_and_mask_keeps_key() {
        let root = test_root("custom_provider_roundtrip_and_mask_keeps_key");
        write_config(
            &root,
            r#"{
              "poller": { "enabled": false },
              "providers": {
                "my-llm": {
                  "display_name": "My LLM",
                  "api_base": "https://api.my-llm.com/v1",
                  "api_key": "real-secret-key",
                  "models": [
                    { "id": "my-model", "display_name": "My Model" }
                  ]
                }
              }
            }"#,
        );
        let registry = ProviderRegistry::new(root.clone());

        // 回显：掩码 + api_key_set=true；poller 键原样保留。
        let view = registry.get_config_view().expect("view should load");
        let custom = view
            .providers
            .iter()
            .find(|p| p.id == "my-llm")
            .expect("custom provider present");
        assert!(custom.api_key_set);
        assert_eq!(custom.api_key.as_deref(), Some("sk-****"));
        assert!(!custom.builtin);

        // 保存（掩码未修改 → key 不被覆盖）。
        let saved = registry
            .save_config(view)
            .expect("save should succeed");
        assert_eq!(saved.providers.iter().find(|p| p.id == "my-llm").unwrap().api_key_set, true);

        let disk = fs::read_to_string(root.join("config.json")).unwrap();
        assert!(disk.contains("real-secret-key"), "masked save must keep key");
        assert!(disk.contains("\"poller\""), "other top-level keys preserved");
    }

    #[test]
    fn save_overwrites_api_key_when_new_value() {
        let root = test_root("save_overwrites_api_key_when_new_value");
        write_config(
            &root,
            r#"{
              "providers": {
                "my-llm": {
                  "api_base": "https://api.my-llm.com/v1",
                  "api_key": "old-key",
                  "models": []
                }
              }
            }"#,
        );
        let registry = ProviderRegistry::new(root.clone());
        let mut view = registry.get_config_view().expect("view should load");
        let custom = view
            .providers
            .iter_mut()
            .find(|p| p.id == "my-llm")
            .unwrap();
        custom.api_key = Some("new-key".to_string());

        registry.save_config(view).expect("save should succeed");
        let disk = fs::read_to_string(root.join("config.json")).unwrap();
        assert!(disk.contains("new-key"));
        assert!(!disk.contains("old-key"));
    }

    #[test]
    fn save_rejects_invalid_id_and_dangling_defaults() {
        let root = test_root("save_rejects_invalid_id_and_dangling_defaults");
        let registry = ProviderRegistry::new(root.clone());

        // 非法字符 id
        let mut view = registry.get_config_view().expect("view should load");
        view.providers[0].id = "bad id!".to_string();
        assert!(registry.save_config(view.clone()).is_err());

        // 指向不存在 provider 的 defaults
        view = registry.get_config_view().expect("view should load");
        view.defaults = Some(ProviderDefaults {
            provider: "ghost".into(),
            model: "m".into(),
        });
        assert!(registry.save_config(view).is_err());
    }

    #[test]
    fn custom_provider_delete_removes_from_disk() {
        let root = test_root("custom_provider_delete_removes_from_disk");
        write_config(
            &root,
            r#"{
              "providers": {
                "my-llm": {
                  "api_base": "https://api.my-llm.com/v1",
                  "models": []
                }
              }
            }"#,
        );
        let registry = ProviderRegistry::new(root.clone());
        let mut view = registry.get_config_view().expect("view should load");
        let custom = view
            .providers
            .iter_mut()
            .find(|p| p.id == "my-llm")
            .unwrap();
        custom.enabled = false;

        registry.save_config(view).expect("save should succeed");
        let disk = fs::read_to_string(root.join("config.json")).unwrap();
        assert!(!disk.contains("my-llm"), "disabled custom provider is removed");
    }

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "pulsar-providers-{name}-{}",
            crate::core::conversation_store::now_ms()
        ))
    }

    fn write_config(root: &PathBuf, config: &str) {
        fs::create_dir_all(root).expect("test config root should be created");
        fs::write(root.join("config.json"), config).expect("test config should be written");
    }
}

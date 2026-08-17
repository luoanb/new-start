use async_trait::async_trait;

use crate::core::{
    error::{AppError, AppResult},
    model_call_input::{ModelAppendTemplate, ModelCallInput},
    models::{GeneratedNeuronDraft, ModelCallRequest, ModelMessage},
    providers::ProviderRegistry,
};

#[async_trait]
pub trait NeuronModelCaller: Send + Sync {
    /// Call the default model with a fully assembled message list.
    async fn call_model(&self, messages: Vec<ModelMessage>) -> AppResult<String>;
}

#[derive(Debug, Clone)]
pub struct DefaultNeuronModelCaller {
    providers: ProviderRegistry,
}

impl DefaultNeuronModelCaller {
    pub fn new(providers: ProviderRegistry) -> Self {
        Self { providers }
    }

    /// Convenience: assemble then call (empty history).
    pub async fn call_assembled(
        &self,
        role_system: &str,
        content: &str,
        user_input: &str,
        template: ModelAppendTemplate,
    ) -> AppResult<String> {
        let wire = ModelCallInput::assemble(&[], role_system, content, user_input, template);
        self.call_model(wire).await
    }
}

#[async_trait]
impl NeuronModelCaller for DefaultNeuronModelCaller {
    async fn call_model(&self, messages: Vec<ModelMessage>) -> AppResult<String> {
        let model = self
            .providers
            .default_model_selection()?
            .ok_or(AppError::ModelNotSelected)?;
        let response = self
            .providers
            .call_model(ModelCallRequest {
                provider_id: model.provider_id,
                model_id: model.model_id,
                messages,
                tools: None,
                params: model.params,
                thinking: model.thinking,
            })
            .await?;
        if response.output.trim().is_empty() {
            return Err(AppError::LlmRequestFailed(
                "Neuron model returned an empty response".into(),
            ));
        }
        Ok(response.output)
    }
}

/// 从 LLM 输出中提取 JSON 对象（宽松解析：先整段尝试，再按第一个 `{` 到最后一个 `}` 截取）。
pub(crate) fn extract_json_object(text: &str) -> AppResult<serde_json::Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if value.is_object() {
            return Ok(value);
        }
    }
    let start = trimmed
        .find('{')
        .ok_or_else(|| AppError::InvalidInput("LLM response missing JSON object".into()))?;
    let end = trimmed
        .rfind('}')
        .ok_or_else(|| AppError::InvalidInput("LLM response missing JSON object end".into()))?;
    if end < start {
        return Err(AppError::InvalidInput(
            "LLM response has invalid JSON object bounds".into(),
        ));
    }
    serde_json::from_str(&trimmed[start..=end])
        .map_err(|e| AppError::InvalidInput(format!("Failed to parse LLM JSON: {e}")))
}

/// 从 LLM 输出中提取 JSON 数组（支持 `{"neurons":[...]}` 包裹形式）。
pub(crate) fn extract_json_array(text: &str) -> AppResult<serde_json::Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if value.is_array() {
            return Ok(value);
        }
        if let Some(neurons) = value.get("neurons").filter(|v| v.is_array()) {
            return Ok(neurons.clone());
        }
    }
    let start = trimmed
        .find('[')
        .ok_or_else(|| AppError::InvalidInput("LLM response missing JSON array".into()))?;
    let end = trimmed
        .rfind(']')
        .ok_or_else(|| AppError::InvalidInput("LLM response missing JSON array end".into()))?;
    if end < start {
        return Err(AppError::InvalidInput(
            "LLM response has invalid JSON array bounds".into(),
        ));
    }
    serde_json::from_str(&trimmed[start..=end])
        .map_err(|e| AppError::InvalidInput(format!("Failed to parse LLM JSON array: {e}")))
}

/// 解析 LLM 生成的神经元草稿列表，严格校验数量与 desc/content 非空。
pub(crate) fn parse_generated_drafts(
    text: &str,
    expected: usize,
) -> AppResult<Vec<GeneratedNeuronDraft>> {
    if expected == 0 {
        return Err(AppError::InvalidInput(
            "expected draft count must be >= 1".into(),
        ));
    }

    let trimmed = text.trim();
    let mut drafts: Vec<GeneratedNeuronDraft> = Vec::new();

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        drafts = drafts_from_json_value(value)?;
    }
    // Prefer a real list before falling back; ignore empty `[]` false-positives
    // (e.g. `"tool_ids":[]` inside a single object).
    if drafts.is_empty() {
        if let Ok(array) = extract_json_array(trimmed) {
            drafts = drafts_from_json_value(array)?;
        }
    }
    if drafts.is_empty() && expected == 1 {
        let value = extract_json_object(trimmed)?;
        drafts = drafts_from_json_value(value)?;
    }
    if drafts.is_empty() {
        return Err(AppError::NeuronBootstrapFailed(
            "Generated neuron list was empty".into(),
        ));
    }
    if drafts.len() > expected {
        drafts.truncate(expected);
    }
    if drafts.len() != expected {
        return Err(AppError::NeuronBootstrapFailed(format!(
            "Expected {expected} generated neuron(s), got {}",
            drafts.len()
        )));
    }
    for draft in &drafts {
        if draft.desc.trim().is_empty() || draft.content.trim().is_empty() {
            return Err(AppError::NeuronBootstrapFailed(
                "Generated neuron must have non-empty desc/content".into(),
            ));
        }
    }
    Ok(drafts)
}

fn drafts_from_json_value(value: serde_json::Value) -> AppResult<Vec<GeneratedNeuronDraft>> {
    match value {
        serde_json::Value::Array(items) => serde_json::from_value(serde_json::Value::Array(items))
            .map_err(|error| {
                AppError::NeuronBootstrapFailed(format!(
                    "Invalid generated neuron list JSON: {error}"
                ))
            }),
        serde_json::Value::Object(map) => {
            if let Some(neurons) = map.get("neurons").cloned() {
                if neurons.is_array() {
                    return drafts_from_json_value(neurons);
                }
            }
            let draft: GeneratedNeuronDraft =
                serde_json::from_value(serde_json::Value::Object(map)).map_err(|error| {
                    AppError::NeuronBootstrapFailed(format!(
                        "Invalid generated neuron JSON: {error}"
                    ))
                })?;
            Ok(vec![draft])
        }
        _ => Ok(Vec::new()),
    }
}

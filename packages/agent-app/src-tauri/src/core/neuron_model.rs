use async_trait::async_trait;

use super::{
    error::{AppError, AppResult},
    models::{ModelCallRequest, ModelMessage, ModelMessageRole},
    providers::ProviderRegistry,
};

#[async_trait]
pub trait NeuronModelCaller: Send + Sync {
    async fn call_model(&self, system_prompt: &str, user_prompt: &str) -> AppResult<String>;
}

#[derive(Debug, Clone)]
pub struct DefaultNeuronModelCaller {
    providers: ProviderRegistry,
}

impl DefaultNeuronModelCaller {
    pub fn new(providers: ProviderRegistry) -> Self {
        Self { providers }
    }
}

#[async_trait]
impl NeuronModelCaller for DefaultNeuronModelCaller {
    async fn call_model(&self, system_prompt: &str, user_prompt: &str) -> AppResult<String> {
        let model = self
            .providers
            .default_model_selection()?
            .ok_or(AppError::ModelNotSelected)?;
        let response = self
            .providers
            .call_model(ModelCallRequest {
                provider_id: model.provider_id,
                model_id: model.model_id,
                messages: vec![
                    ModelMessage {
                        role: ModelMessageRole::System,
                        content: system_prompt.to_string(),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                    ModelMessage {
                        role: ModelMessageRole::User,
                        content: user_prompt.to_string(),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                ],
                tools: None,
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

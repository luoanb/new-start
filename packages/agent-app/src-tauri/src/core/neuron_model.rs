use async_trait::async_trait;

use super::{
    error::{AppError, AppResult},
    model_call_input::{ModelAppendTemplate, ModelCallInput},
    models::{ModelCallRequest, ModelMessage},
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
        let messages = ModelCallInput::assemble(&[], role_system, content, user_input, template);
        self.call_model(messages).await
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

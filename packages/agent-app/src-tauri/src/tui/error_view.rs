use crate::core::AppError;

#[derive(Debug, Clone)]
pub struct TuiErrorView {
    pub code: String,
    pub what_happened: String,
    pub possible_causes: Vec<String>,
    pub next_actions: Vec<String>,
    pub raw_summary: Option<String>,
}

impl From<AppError> for TuiErrorView {
    fn from(error: AppError) -> Self {
        let code = error.code().to_string();
        let raw = error.to_string();

        let (what_happened, possible_causes, next_actions) = match &error {
            AppError::ModelNotSelected => (
                "No active provider/model is selected for chat.".into(),
                vec![
                    "No default model configured in config.json.".into(),
                    "No /provider or /model command has been run.".into(),
                ],
                vec![
                    "Run /provider to list available providers.".into(),
                    "Run /model <provider> <model> to select a model.".into(),
                ],
            ),
            AppError::ProviderAuthMissing(provider_id) => (
                format!("Authentication credentials missing for provider `{provider_id}`."),
                vec![
                    format!("Environment variable for {provider_id} API key is not set."),
                    "API key may be missing from config.json.".into(),
                ],
                vec![
                    format!(
                        "Set the {provider_id} API key via environment variable or config.json."
                    ),
                    format!("Check documentation for required auth env: `{provider_id}`."),
                ],
            ),
            AppError::ModelNotFound(model_id) => (
                format!("Model `{model_id}` is not configured."),
                vec![
                    "The model ID was not found in the provider's model list.".into(),
                    "The model might be misspelled or not yet added to config.".into(),
                ],
                vec![
                    "Run /models <provider> to see available models.".into(),
                    "Add the model to providers.<id>.models in config.json.".into(),
                ],
            ),
            AppError::LlmRequestFailed(_) => (
                "The provider returned an error or could not be reached.".into(),
                vec![
                    "The model name might be incorrect.".into(),
                    "The API base URL might be misconfigured.".into(),
                    "The provider credentials might be invalid.".into(),
                ],
                vec![
                    "Check the model name and API base URL.".into(),
                    "Verify provider credentials are correct.".into(),
                    "Run /status to view current configuration.".into(),
                ],
            ),
            AppError::InvalidInput(msg) => (
                msg.clone(),
                vec![
                    "The command format may be wrong.".into(),
                    "The message may contain invalid characters.".into(),
                ],
                vec!["Run /help to see available commands and usage.".into()],
            ),
            AppError::ProviderNotFound(provider_id) => (
                format!("Provider `{provider_id}` is not registered."),
                vec![
                    "The provider ID might be misspelled.".into(),
                    "The provider might not be configured in the system.".into(),
                ],
                vec!["Run /provider to list all registered providers.".into()],
            ),
            AppError::ConversationNotFound(id) => (
                format!("Conversation `{id}` was not found."),
                vec![
                    "The session may have been deleted.".into(),
                    "The session ID might be incorrect.".into(),
                ],
                vec!["Run /sessions to list available conversations.".into()],
            ),
            AppError::NeuronNotFound(id) => (
                format!("Neuron `{id}` was not found."),
                vec![
                    "The neuron may have been deleted.".into(),
                    "The neuron ID might be incorrect.".into(),
                ],
                vec!["Run /neuron list to list available neurons.".into()],
            ),
            AppError::NeuronBootstrapFailed(_) => (
                "Neuron bootstrap failed.".into(),
                vec![
                    "The bootstrap prompt may be missing or invalid.".into(),
                    "The model may have returned invalid neuron JSON.".into(),
                ],
                vec![
                    "Check neurons.bootstrap.create_neuron_prompt in config.json.".into(),
                    "Run /neuron ensure-creator or /neuron bootstrap to diagnose.".into(),
                ],
            ),
            AppError::SkillNotFound(name) => (
                format!("Skill `{name}` was not found."),
                vec!["The skill name might be misspelled.".into()],
                vec!["Run /skills to list available skills.".into()],
            ),
            AppError::StorageError(_) => (
                "A storage operation failed.".into(),
                vec![
                    "The storage directory may be corrupted.".into(),
                    "There may be a file permission issue.".into(),
                ],
                vec![
                    "Check storage path in /status.".into(),
                    "Verify file permissions on the data directory.".into(),
                ],
            ),
            AppError::CompactionFailed(_) => (
                "Conversation compaction failed.".into(),
                vec![
                    "The LLM call for summarization may have failed.".into(),
                    "The conversation might contain too few messages.".into(),
                ],
                vec![
                    "Check that the selected model is available.".into(),
                    "Ensure the conversation has enough messages to compact.".into(),
                ],
            ),
            AppError::AgentMaxIterations(_) => (
                "Agent stopped: maximum tool call iterations reached.".into(),
                vec![
                    "The model kept requesting tool calls without producing a final answer.".into(),
                    "A tool may be returning unexpected results, causing repeated calls.".into(),
                ],
                vec![
                    "Try simplifying your request.".into(),
                    "Check that the tools are returning correct results.".into(),
                ],
            ),
            AppError::RuntimeError(_) => (
                "An internal runtime error occurred.".into(),
                vec!["An unexpected error occurred in the application logic.".into()],
                vec!["Restart the application. If the problem persists, check logs.".into()],
            ),
        };

        Self {
            code,
            what_happened,
            possible_causes,
            next_actions,
            raw_summary: Some(raw),
        }
    }
}

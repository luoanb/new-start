use crate::core::{Conversation, ModelInfo, ProviderInfo};

#[derive(Debug)]
pub enum Command {
    Help,
    New,
    Skills,
    Providers,
    Sessions,
    History,
    Clear,
    Status,
    Model(String, String),
    Provider(String),
    Models(String),
    Call(String, String, String),
    Config,
    Exit,
}

impl Command {
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }

        match trimmed {
            "/help" => Some(Self::Help),
            "/new" => Some(Self::New),
            "/skills" => Some(Self::Skills),
            "/providers" => Some(Self::Providers),
            "/sessions" => Some(Self::Sessions),
            "/history" => Some(Self::History),
            "/clear" => Some(Self::Clear),
            "/status" => Some(Self::Status),
            "/config" => Some(Self::Config),
            "/exit" | "/quit" => Some(Self::Exit),
            _ => {
                let parts: Vec<&str> = trimmed.splitn(4, ' ').collect();
                match parts[0] {
                    "/model" if parts.len() >= 3 => Some(Self::Model(
                        parts[1].to_string(),
                        parts[2].to_string(),
                    )),
                    "/provider" if parts.len() >= 2 => {
                        Some(Self::Provider(parts[1].to_string()))
                    }
                    "/models" if parts.len() >= 2 => {
                        Some(Self::Models(parts[1].to_string()))
                    }
                    "/call" if parts.len() >= 4 => Some(Self::Call(
                        parts[1].to_string(),
                        parts[2].to_string(),
                        parts[3..].join(" "),
                    )),
                    _ => None,
                }
            }
        }
    }
}

pub fn cmd_help_text() -> Vec<(String, String)> {
    vec![
        ("/help".into(), "Show this help".into()),
        ("/new".into(), "Create a new blank session".into()),
        ("/skills".into(), "List available skills".into()),
        ("/providers".into(), "List providers".into()),
        ("/provider <id>".into(), "Show provider details".into()),
        ("/models <provider>".into(), "List models for provider".into()),
        (
            "/model <provider> <model>".into(),
            "Select active chat model".into(),
        ),
        ("/sessions".into(), "List conversations".into()),
        ("/history".into(), "Show current conversation history".into()),
        ("/clear".into(), "Clear current conversation".into()),
        ("/status".into(), "Show runtime status".into()),
        ("/config".into(), "Show configuration".into()),
        ("/call <p> <m> <msg>".into(), "Call a model directly".into()),
        ("/exit".into(), "Quit the application".into()),
    ]
}

pub fn cmd_provider_text(providers: &[ProviderInfo]) -> String {
    if providers.is_empty() {
        return "  no providers configured".to_string();
    }

    providers
        .iter()
        .map(|p| {
            format!(
                "  {} - {} (auth: {})",
                p.id, p.display_name, p.auth_env
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn cmd_models_text(models: &[ModelInfo]) -> String {
    if models.is_empty() {
        return "  no configured models".to_string();
    }

    models
        .iter()
        .map(|m| {
            format!(
                "  {} (provider: {}, chat: {}, tools: {}, streaming: {})",
                m.id,
                m.provider_id,
                m.capabilities.chat,
                m.capabilities.tools,
                m.capabilities.streaming
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn cmd_sessions_text(conversations: &[Conversation]) -> String {
    if conversations.is_empty() {
        return "  no conversations".to_string();
    }

    conversations
        .iter()
        .map(|c| {
            format!(
                "  {} ({} messages, updated: {})",
                c.id,
                c.messages.len(),
                c.updated_at
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

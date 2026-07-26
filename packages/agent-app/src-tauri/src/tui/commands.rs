use crate::core::{Conversation, ModelInfo, ProviderInfo};

#[derive(Debug)]
pub enum Command {
    Help,
    New,
    NewAgent,
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
    Compact,
    Agent(String),
    TopicAction(Vec<String>),
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
            "/compact" => Some(Self::Compact),
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
                    "/new" if parts.len() >= 2 && parts[1] == "agent" => {
                        Some(Self::NewAgent)
                    }
                    "/agent" if parts.len() >= 2 => {
                        Some(Self::Agent(parts[1..].join(" ")))
                    }
                    "/topic" if parts.len() >= 1 => {
                        Some(Self::TopicAction(parts[1..].iter().map(|s| s.to_string()).collect()))
                    }
                    _ => None,
                }
            }
        }
    }
}

pub fn cmd_help_text() -> Vec<(String, String)> {
    vec![
        ("/help".into(), "Show this help".into()),
        ("/new".into(), "Create a new Chat session".into()),
        ("/new agent".into(), "Create a new Agent session".into()),
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
        ("/compact".into(), "Manually compress current conversation".into()),
        ("/call <p> <m> <msg>".into(), "Call a model directly".into()),
        ("/agent <message>".into(), "Send a message with tool-calling agent loop".into()),
        ("/topic <cmd>".into(), "Topic management: list, new, <id>, <id> set, <id> delete (use /topic alone for help)".into()),
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
            let mut out = format!("  {}", m.id);

            let ctx = m
                .context_window
                .map_or("-".to_string(), |v| format!("{v} ctx"));
            let max_out = m
                .max_output_tokens
                .map_or("-".to_string(), |v| format!("{v} max out"));
            out.push_str(&format!("\n    capacity: {ctx} | {max_out}"));

            if let (Some(i), Some(o)) = (m.pricing_input, m.pricing_output) {
                out.push_str(&format!("\n    pricing : ${i:.2}/M in, ${o:.2}/M out"));
            } else if let Some(i) = m.pricing_input {
                out.push_str(&format!("\n    pricing : ${i:.2}/M in"));
            } else if let Some(o) = m.pricing_output {
                out.push_str(&format!("\n    pricing : ${o:.2}/M out"));
            }

            let caps = {
                let mut parts = vec![];
                if m.capabilities.chat {
                    parts.push("chat");
                }
                if m.capabilities.tools {
                    parts.push("tools");
                }
                if m.capabilities.streaming {
                    parts.push("streaming");
                }
                if m.capabilities.structured_output {
                    parts.push("json");
                }
                if m.capabilities.vision.unwrap_or(false) {
                    parts.push("vision");
                }
                parts
            };
            if !caps.is_empty() {
                out.push_str(&format!("\n    features: {}", caps.join(" \u{2713} ")));
                out.push_str(" \u{2713}");
            }

            if let Some(extras) = &m.capabilities.extras {
                if !extras.is_empty() {
                    let items: Vec<String> = extras
                        .iter()
                        .map(|(k, v)| format!("{k}: {v}"))
                        .collect();
                    out.push_str(&format!("\n    extras  : {}", items.join(", ")));
                }
            }

            out
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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

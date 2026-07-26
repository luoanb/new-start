use super::{
    conversation_store::now_ms,
    error::{AppError, AppResult},
    models::{Message, MessageRole},
    skills::SkillRegistry,
};

#[derive(Debug, Clone)]
pub struct AgentRuntime {
    skills: SkillRegistry,
}

impl AgentRuntime {
    pub fn new(skills: SkillRegistry) -> Self {
        Self { skills }
    }

    pub fn respond(&self, input: &str) -> AppResult<Message> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(AppError::InvalidInput("Message cannot be empty".into()));
        }

        let response = if let Some(message) = trimmed.strip_prefix("/echo ") {
            self.skills.execute_echo(message.trim())?
        } else if trimmed == "/time" {
            format!("Current timestamp: {}", self.skills.execute_time()?)
        } else {
            format!("Agent App 收到：{trimmed}")
        };

        if response.trim().is_empty() {
            return Err(AppError::RuntimeError(
                "Runtime produced an empty response".into(),
            ));
        }

        Ok(Message {
            role: MessageRole::Assistant,
            content: response,
            timestamp: now_ms(),
            msg_type: None,
            summary_of: None,
            tool_calls: None,
            tool_call_id: None,
        })
    }
}

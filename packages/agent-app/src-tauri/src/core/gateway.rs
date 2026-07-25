use super::{
    error::{AppError, AppResult},
    models::{ChatResponse, Conversation, Message, MessageRole, RuntimeStatus, SkillInfo},
    runtime::AgentRuntime,
    skills::SkillRegistry,
    storage::{now_ms, Storage},
};

#[derive(Debug, Clone)]
pub struct Gateway {
    storage: Storage,
    skills: SkillRegistry,
    runtime: AgentRuntime,
    current_conversation_id: String,
}

impl Gateway {
    pub fn default() -> AppResult<Self> {
        Self::new(Storage::default()?)
    }

    pub fn new(storage: Storage) -> AppResult<Self> {
        let skills = SkillRegistry::with_defaults();
        let runtime = AgentRuntime::new(skills.clone());
        let current_conversation_id = match storage.list_conversations()?.first() {
            Some(conversation) => conversation.id.clone(),
            None => storage.create_conversation(None)?.id,
        };

        Ok(Self {
            storage,
            skills,
            runtime,
            current_conversation_id,
        })
    }

    pub fn send_message(
        &mut self,
        input: impl AsRef<str>,
        conversation_id: Option<String>,
    ) -> AppResult<ChatResponse> {
        let input = input.as_ref().trim();
        if input.is_empty() {
            return Err(AppError::InvalidInput("Message cannot be empty".into()));
        }

        let conversation_id = self.resolve_conversation_id(conversation_id)?;
        let user_message = Message {
            role: MessageRole::User,
            content: input.to_string(),
            timestamp: now_ms(),
        };

        self.storage.add_message(&conversation_id, user_message)?;
        let assistant_message = self.runtime.respond(input)?;
        let response = assistant_message.content.clone();
        self.storage
            .add_message(&conversation_id, assistant_message)?;
        self.current_conversation_id = conversation_id.clone();

        Ok(ChatResponse {
            conversation_id,
            response,
        })
    }

    pub fn list_skills(&self) -> Vec<SkillInfo> {
        self.skills.list()
    }

    pub fn list_conversations(&self) -> AppResult<Vec<Conversation>> {
        self.storage.list_conversations()
    }

    pub fn history(&self, conversation_id: Option<String>) -> AppResult<Vec<Message>> {
        let conversation_id = self.resolve_existing_conversation_id(conversation_id)?;
        Ok(self
            .storage
            .require_conversation(&conversation_id)?
            .messages)
    }

    pub fn clear_conversation(&mut self, conversation_id: Option<String>) -> AppResult<String> {
        let conversation_id = self.resolve_existing_conversation_id(conversation_id)?;
        self.storage.clear_conversation(&conversation_id)?;

        if self.current_conversation_id == conversation_id {
            self.current_conversation_id = self.storage.create_conversation(None)?.id;
        }

        Ok(conversation_id)
    }

    pub fn status(&self) -> AppResult<RuntimeStatus> {
        Ok(RuntimeStatus {
            app_name: "agent-app".to_string(),
            storage_path: self.storage.root().display().to_string(),
            current_conversation_id: self.current_conversation_id.clone(),
            skill_count: self.skills.list().len(),
            conversation_count: self.storage.list_conversations()?.len(),
        })
    }

    fn resolve_conversation_id(&mut self, conversation_id: Option<String>) -> AppResult<String> {
        match conversation_id {
            Some(id) if id.trim().is_empty() => Err(AppError::InvalidInput(
                "Conversation id cannot be empty".into(),
            )),
            Some(id) => {
                if self.storage.get_conversation(&id)?.is_none() {
                    self.storage.create_conversation(Some(id.clone()))?;
                }
                Ok(id)
            }
            None => Ok(self.current_conversation_id.clone()),
        }
    }

    fn resolve_existing_conversation_id(
        &self,
        conversation_id: Option<String>,
    ) -> AppResult<String> {
        let id = conversation_id.unwrap_or_else(|| self.current_conversation_id.clone());
        if id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Conversation id cannot be empty".into(),
            ));
        }

        self.storage.require_conversation(&id)?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn send_message_persists_user_and_assistant_messages() {
        let mut gateway = test_gateway("send_message_persists_user_and_assistant_messages");

        let response = gateway
            .send_message("hello", None)
            .expect("message should be accepted");
        let history = gateway
            .history(Some(response.conversation_id))
            .expect("history should load");

        assert_eq!(response.response, "Agent App 收到：hello");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, MessageRole::User);
        assert_eq!(history[1].role, MessageRole::Assistant);
    }

    #[test]
    fn list_skills_returns_default_registry() {
        let gateway = test_gateway("list_skills_returns_default_registry");
        let skill_names = gateway
            .list_skills()
            .into_iter()
            .map(|skill| skill.name)
            .collect::<Vec<_>>();

        assert_eq!(skill_names, vec!["calculate", "echo", "get_current_time"]);
    }

    #[test]
    fn clear_conversation_removes_selected_session() {
        let mut gateway = test_gateway("clear_conversation_removes_selected_session");
        let response = gateway
            .send_message("hello", None)
            .expect("message should be accepted");

        let cleared = gateway
            .clear_conversation(Some(response.conversation_id.clone()))
            .expect("conversation should clear");

        assert_eq!(cleared, response.conversation_id);
        assert!(gateway.history(Some(cleared)).is_err());
    }

    fn test_gateway(name: &str) -> Gateway {
        let root = test_root(name);
        if root.exists() {
            fs::remove_dir_all(&root).expect("old test storage should be removable");
        }

        let storage = Storage::new(root).expect("test storage should initialize");
        Gateway::new(storage).expect("test gateway should initialize")
    }

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agent-app-{name}-{}", now_ms()))
    }
}

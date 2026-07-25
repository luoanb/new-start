use super::{
    conversation_store::{now_ms, ConversationStore},
    engine::Engine,
    error::{AppError, AppResult},
    models::{
        ChatModelSelection, ChatOptions, ChatResponse, Conversation, Message, MessageRole,
        ModelCallRequest, ModelCallResponse, ModelInfo, ProviderInfo, RuntimeStatus, SkillInfo,
    },
    providers::ProviderRegistry,
    skills::SkillRegistry,
    CompactionConfig,
};

#[derive(Debug, Clone)]
pub struct Gateway {
    engine: Engine,
    store: ConversationStore,
    providers: ProviderRegistry,
    skills: SkillRegistry,
    current_conversation_id: String,
}

impl Gateway {
    pub fn default() -> AppResult<Self> {
        Self::new(ConversationStore::default()?)
    }

    pub fn new(store: ConversationStore) -> AppResult<Self> {
        let skills = SkillRegistry::with_defaults();
        let providers = ProviderRegistry::new(store.root().to_path_buf());
        let current_conversation_id = match store.list_conversations()?.first() {
            Some(conversation) => conversation.id.clone(),
            None => store.create_conversation(None)?.id,
        };
        let engine = Engine::new(
            store.clone(),
            providers.clone(),
            CompactionConfig::default(),
        );

        Ok(Self {
            engine,
            store,
            skills,
            providers,
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
            msg_type: None,
            summary_of: None,
        };

        self.store.add_message(&conversation_id, user_message)?;
        let assistant_message = self.runtime_respond(input)?;
        let response = assistant_message.content.clone();
        self.store
            .add_message(&conversation_id, assistant_message)?;
        self.current_conversation_id = conversation_id.clone();

        Ok(ChatResponse {
            conversation_id,
            response,
        })
    }

    /// Quick inline runtime respond for built-in commands.
    fn runtime_respond(&self, input: &str) -> AppResult<Message> {
        let trimmed = input.trim();
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
        })
    }

    pub fn list_skills(&self) -> Vec<SkillInfo> {
        self.skills.list()
    }

    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        self.providers.list_providers()
    }

    pub fn list_models(&self, provider_id: Option<String>) -> AppResult<Vec<ModelInfo>> {
        self.providers.list_models(provider_id.as_deref())
    }

    pub fn default_model_selection(&self) -> AppResult<Option<ChatModelSelection>> {
        self.providers.default_model_selection()
    }

    pub fn require_model(&self, provider_id: &str, model_id: &str) -> AppResult<()> {
        self.providers.require_model(provider_id, model_id)
    }

    pub async fn call_model(&self, request: ModelCallRequest) -> AppResult<ModelCallResponse> {
        self.providers.call_model(request).await
    }

    pub async fn send_model_message(
        &mut self,
        input: impl AsRef<str>,
        options: ChatOptions,
    ) -> AppResult<ChatResponse> {
        let input = input.as_ref().trim();
        if input.is_empty() {
            return Err(AppError::InvalidInput("Message cannot be empty".into()));
        }

        self.providers
            .require_model(&options.provider_id, &options.model_id)?;

        let conversation_id = self.resolve_conversation_id(options.conversation_id.clone())?;

        // Delegate to engine for orchestration (get conversation → compact → call model → save)
        let response = self
            .engine
            .chat(input, conversation_id.clone(), options)
            .await?;

        self.current_conversation_id = conversation_id.clone();

        Ok(response)
    }

    /// Manually trigger compaction for the current conversation.
    pub async fn compact_conversation(
        &mut self,
        conversation_id: Option<String>,
    ) -> AppResult<String> {
        let id = self.resolve_existing_conversation_id(conversation_id)?;
        let model = self
            .default_model_selection()?
            .ok_or(AppError::ModelNotSelected)?;
        self.engine.compact(&id, &model).await?;
        Ok(format!("Compacted conversation {id}"))
    }

    pub fn list_conversations(&self) -> AppResult<Vec<Conversation>> {
        self.store.list_conversations()
    }

    pub fn history(&self, conversation_id: Option<String>) -> AppResult<Vec<Message>> {
        let conversation_id = self.resolve_existing_conversation_id(conversation_id)?;
        Ok(self.store.require_conversation(&conversation_id)?.messages)
    }

    pub fn clear_conversation(&mut self, conversation_id: Option<String>) -> AppResult<String> {
        let conversation_id = self.resolve_existing_conversation_id(conversation_id)?;
        self.store.clear_conversation(&conversation_id)?;

        if self.current_conversation_id == conversation_id {
            self.current_conversation_id = self.store.create_conversation(None)?.id;
        }

        Ok(conversation_id)
    }

    /// Create a new blank conversation and return its id.
    /// The current conversation is left unchanged.
    pub fn create_new_conversation(&mut self) -> AppResult<String> {
        let conv = self.store.create_conversation(None)?;
        Ok(conv.id)
    }

    pub fn status(&self) -> AppResult<RuntimeStatus> {
        Ok(RuntimeStatus {
            app_name: "agent-app".to_string(),
            storage_path: self.store.root().display().to_string(),
            current_conversation_id: self.current_conversation_id.clone(),
            skill_count: self.skills.list().len(),
            conversation_count: self.store.list_conversations()?.len(),
        })
    }

    fn resolve_conversation_id(&mut self, conversation_id: Option<String>) -> AppResult<String> {
        match conversation_id {
            Some(id) if id.trim().is_empty() => Err(AppError::InvalidInput(
                "Conversation id cannot be empty".into(),
            )),
            Some(id) => {
                if self.store.get_conversation(&id)?.is_none() {
                    self.store.create_conversation(Some(id.clone()))?;
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

        self.store.require_conversation(&id)?;
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

    #[tokio::test]
    async fn send_model_message_rejects_missing_model_without_history_write() {
        let mut gateway =
            test_gateway("send_model_message_rejects_missing_model_without_history_write");
        let conversation_id = gateway.current_conversation_id.clone();

        let error = gateway
            .send_model_message(
                "hello",
                ChatOptions {
                    provider_id: "deepseek".to_string(),
                    model_id: "missing-model".to_string(),
                    conversation_id: None,
                },
            )
            .await
            .expect_err("missing model should be rejected");
        let history = gateway
            .history(Some(conversation_id))
            .expect("history should still load");

        assert_eq!(error.code(), "model_not_found");
        assert!(history.is_empty());
    }

    fn test_gateway(name: &str) -> Gateway {
        let root = test_root(name);
        if root.exists() {
            fs::remove_dir_all(&root).expect("old test storage should be removable");
        }

        let store = ConversationStore::new(root).expect("test store should initialize");
        Gateway::new(store).expect("test gateway should initialize")
    }

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agent-app-{name}-{}", now_ms()))
    }
}

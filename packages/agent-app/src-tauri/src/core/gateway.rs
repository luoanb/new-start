use std::sync::{Arc, Mutex};

use super::{
    conversation_store::{now_ms, ConversationStore},
    engine::Engine,
    error::{AppError, AppResult},
    models::{
        ChatModelSelection, ChatOptions, ChatResponse, Conversation, ConversationMode, Message,
        MessageRole, ModelCallRequest, ModelCallResponse, ModelInfo, ProviderInfo, RuntimeStatus,
        SkillInfo,
    },
    neuron_config::NeuronConfigReader,
    neuron_manager::NeuronManager,
    neuron_model::DefaultNeuronModelCaller,
    neuron_store::NeuronStore,
    providers::ProviderRegistry,
    session_tracker::SessionTracker,
    tool_registry::ToolRegistry,
    topic_store::TopicStore,
    CompactionConfig,
};

#[derive(Debug, Clone)]
pub struct Gateway {
    engine: Engine,
    store: ConversationStore,
    providers: ProviderRegistry,
    tool_registry: Option<ToolRegistry>,
    topic_store: Option<Arc<Mutex<TopicStore>>>,
    neuron_store: Option<Arc<Mutex<NeuronStore>>>,
    neuron_manager: Arc<NeuronManager>,
    session_tracker: SessionTracker,
    current_conversation_id: String,
}

impl Gateway {
    pub fn default() -> AppResult<Self> {
        Self::new(ConversationStore::default()?)
    }

    pub fn new(store: ConversationStore) -> AppResult<Self> {
        let providers = ProviderRegistry::new(store.root().to_path_buf());
        let current_conversation_id = match store.list_conversations()?.first() {
            Some(conversation) => conversation.id.clone(),
            None => store.create_conversation(None, ConversationMode::Chat)?.id,
        };

        // Initialize App-level SQLite database
        let db_path = store.root().join("app.db");
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| AppError::StorageError(format!("Failed to open app.db: {}", e)))?;
        let conn = Arc::new(Mutex::new(conn));

        let topic_store = Arc::new(Mutex::new(TopicStore::new(Arc::clone(&conn))));
        topic_store
            .lock()
            .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?
            .init_table()?;

        let neuron_store = Arc::new(Mutex::new(NeuronStore::new(Arc::clone(&conn))));
        neuron_store
            .lock()
            .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?
            .init_table()?;

        let session_tracker = SessionTracker::new();
        let neuron_manager = Arc::new(NeuronManager::new(
            Arc::clone(&neuron_store),
            Arc::new(DefaultNeuronModelCaller::new(providers.clone())),
            NeuronConfigReader::new(store.root().to_path_buf()),
        ));

        let tool_registry = Some(ToolRegistry::with_defaults_and_topics_and_neurons(
            Arc::clone(&topic_store),
            Arc::clone(&neuron_manager),
            session_tracker.clone(),
        ));
        let engine = Engine::with_tools(
            store.clone(),
            providers.clone(),
            CompactionConfig::default(),
            tool_registry.clone().unwrap(),
        );

        Ok(Self {
            engine,
            store,
            providers,
            tool_registry,
            topic_store: Some(topic_store),
            neuron_store: Some(neuron_store),
            neuron_manager,
            session_tracker: session_tracker,
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
            tool_calls: None,
            tool_call_id: None,
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
            message.trim().to_string()
        } else if trimmed == "/time" {
            format!("Current timestamp: {}", now_ms())
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

    pub fn list_skills(&self) -> Vec<SkillInfo> {
        self.tool_registry
            .as_ref()
            .map(|reg| {
                reg.list_definitions()
                    .into_iter()
                    .map(|d| SkillInfo {
                        name: d.name,
                        description: d.description,
                    })
                    .collect()
            })
            .unwrap_or_default()
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

        // Register as a running session
        self.session_tracker.register(&conversation_id, None)?;

        // Engine dispatches by conversation.mode internally
        let result = self
            .engine
            .chat(input, conversation_id.clone(), options)
            .await;

        // Unregister on completion (success or error)
        self.session_tracker.unregister(&conversation_id);

        let response = result?;

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
            self.current_conversation_id = self
                .store
                .create_conversation(None, ConversationMode::Chat)?
                .id;
        }

        Ok(conversation_id)
    }

    /// Create a new blank conversation with the given mode and return its id.
    /// The current conversation is left unchanged.
    pub fn create_new_conversation(&mut self, mode: ConversationMode) -> AppResult<String> {
        let conv = self.store.create_conversation(None, mode)?;
        Ok(conv.id)
    }

    pub fn status(&self) -> AppResult<RuntimeStatus> {
        Ok(RuntimeStatus {
            app_name: "agent-app".to_string(),
            storage_path: self.store.root().display().to_string(),
            current_conversation_id: self.current_conversation_id.clone(),
            skill_count: self
                .tool_registry
                .as_ref()
                .map(|r| r.list_definitions().len())
                .unwrap_or(0),
            conversation_count: self.store.list_conversations()?.len(),
        })
    }

    /// Access the TopicStore for TUI commands.
    pub fn topic_store(&self) -> AppResult<Arc<Mutex<TopicStore>>> {
        self.topic_store
            .clone()
            .ok_or_else(|| AppError::StorageError("TopicStore not initialized".into()))
    }

    /// Access the NeuronStore for TUI commands.
    pub fn neuron_store(&self) -> AppResult<Arc<Mutex<NeuronStore>>> {
        self.neuron_store
            .clone()
            .ok_or_else(|| AppError::StorageError("NeuronStore not initialized".into()))
    }

    pub fn neuron_manager(&self) -> Arc<NeuronManager> {
        Arc::clone(&self.neuron_manager)
    }

    /// Access the SessionTracker for TUI commands.
    pub fn session_tracker(&self) -> SessionTracker {
        self.session_tracker.clone()
    }

    fn resolve_conversation_id(&mut self, conversation_id: Option<String>) -> AppResult<String> {
        match conversation_id {
            Some(id) if id.trim().is_empty() => Err(AppError::InvalidInput(
                "Conversation id cannot be empty".into(),
            )),
            Some(id) => {
                if self.store.get_conversation(&id)?.is_none() {
                    self.store
                        .create_conversation(Some(id.clone()), ConversationMode::Chat)?;
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
    fn list_skills_returns_tool_registry() {
        let gateway = test_gateway("list_skills_returns_tool_registry");
        let skill_names = gateway
            .list_skills()
            .into_iter()
            .map(|skill| skill.name)
            .collect::<Vec<_>>();

        assert!(
            skill_names.len() > 3,
            "expected many tools, got: {:?}",
            skill_names
        );
        assert!(skill_names.contains(&"get_current_time".to_string()));
        assert!(skill_names.contains(&"echo".to_string()));
        assert!(skill_names.contains(&"create_downstream_neuron".to_string()));
        assert!(skill_names.contains(&"select_neuron_candidates".to_string()));
        assert!(!skill_names.contains(&"create_neuron".to_string()));
        assert!(skill_names.contains(&"add_topic_scope_item".to_string()));
        assert!(skill_names.contains(&"delete_topic_scope_item".to_string()));
        assert!(skill_names.contains(&"complete_topic_scope_item".to_string()));
        assert!(skill_names.contains(&"pause_topic".to_string()));
        assert!(skill_names.contains(&"resume_topic".to_string()));
        assert!(skill_names.contains(&"get_running_sessions".to_string()));
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

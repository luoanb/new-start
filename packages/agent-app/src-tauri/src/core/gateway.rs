use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri;
use tokio::sync::mpsc;

use super::{
    assistant_mode::{AssistantMode, AssistantStepRequest},
    cmd_exec::ExecuteCommandTool,
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
    poller::{Poller, PollerConfigReader, PollerStatus},
    providers::ProviderRegistry,
    session_tracker::SessionTracker,
    tool_registry::ToolRegistry,
    topic_store::TopicStore,
    CompactionConfig,
};

use super::events::{StateChange, StateEmitter};

#[derive(Debug, Clone)]
pub struct Gateway {
    engine: Engine,
    store: ConversationStore,
    providers: ProviderRegistry,
    tool_registry: Option<ToolRegistry>,
    topic_store: Option<Arc<Mutex<TopicStore>>>,
    neuron_store: Option<Arc<Mutex<NeuronStore>>>,
    neuron_manager: Arc<NeuronManager>,
    assistant: Arc<AssistantMode>,
    poller: Arc<Mutex<Poller>>,
    session_tracker: SessionTracker,
    /// Shared so Gateway can be used via `&self` / Tauri State without holding an outer lock across await.
    current_conversation_id: Arc<Mutex<String>>,
}

impl Gateway {
    pub fn default() -> AppResult<Self> {
        Self::new(ConversationStore::default()?)
    }

    pub fn new(store: ConversationStore) -> AppResult<Self> {
        Self::with_state_emitter(store, None)
    }

    /// 与 `new` 等价，但允许注入状态事件发射器，
    /// 用于 Tauri 运行时向前端广播状态变更。
    pub fn with_state_emitter(
        store: ConversationStore,
        state_emit: Option<StateEmitter>,
    ) -> AppResult<Self> {
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
        if let Some(emit) = state_emit.as_ref() {
            let emit = Arc::clone(emit);
            session_tracker.set_on_change(Arc::new(move || {
                emit(StateChange::Sessions);
            }));
        }

        // Production tools are registered only when they have self-describing inserts.
        // `execute_command` is the first on-shelf tool (see inserts/execute_command.md).
        let mut tool_registry = ToolRegistry::new();
        tool_registry.register(ExecuteCommandTool::new());

        let neuron_manager = Arc::new(NeuronManager::new(
            Arc::clone(&neuron_store),
            Arc::new(DefaultNeuronModelCaller::new(providers.clone())),
            NeuronConfigReader::new(store.root().to_path_buf()),
            tool_registry.clone(),
        ));
        let engine = Engine::with_tools(
            store.clone(),
            providers.clone(),
            CompactionConfig::default(),
            tool_registry.clone(),
        );

        let (step_tx, step_rx) = mpsc::unbounded_channel::<AssistantStepRequest>();
        let assistant = Arc::new(AssistantMode::new(
            store.clone(),
            providers.clone(),
            Arc::clone(&neuron_manager),
            Arc::clone(&topic_store),
            Arc::clone(&neuron_store),
            tool_registry.clone(),
            step_tx,
            session_tracker.clone(),
        ));

        let poller_settings = PollerConfigReader::new(store.root().to_path_buf()).load()?;
        tracing::info!(
            phase = "poller_config",
            enabled = poller_settings.enabled,
            base_interval_ms = poller_settings.base_interval_ms,
            assistant_interval_ticks = poller_settings.assistant_interval_ticks,
            "loaded poller settings from config"
        );

        let poller = Arc::new(Mutex::new(Poller::new(poller_settings.base_interval_ms)));
        {
            let mut guard = poller
                .lock()
                .map_err(|e| AppError::StorageError(format!("Poller lock error: {}", e)))?;
            assistant.register_polling(&mut guard, poller_settings.assistant_interval_ticks)?;
            if poller_settings.enabled {
                guard.resume();
            } else {
                guard.pause();
            }
        }

        spawn_poller_runtime(
            Arc::clone(&poller),
            Arc::clone(&assistant),
            providers.clone(),
            step_rx,
            poller_settings.base_interval_ms,
            state_emit,
        );

        Ok(Self {
            engine,
            store,
            providers,
            tool_registry: Some(tool_registry),
            topic_store: Some(topic_store),
            neuron_store: Some(neuron_store),
            neuron_manager,
            assistant,
            poller,
            session_tracker,
            current_conversation_id: Arc::new(Mutex::new(current_conversation_id)),
        })
    }

    pub fn send_message(
        &self,
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
        self.set_current_conversation_id(conversation_id.clone())?;

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
        &self,
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
        let conversation = self.store.require_conversation(&conversation_id)?;
        let mode = conversation.mode;

        // Clone handles before any network await — callers must not hold an outer Gateway lock.
        let assistant = Arc::clone(&self.assistant);
        let engine = self.engine.clone();
        let session_tracker = self.session_tracker.clone();
        session_tracker.register(&conversation_id, None)?;

        let result = if mode == ConversationMode::Assistant {
            tracing::info!(
                phase = "send_model_message",
                mode = "assistant",
                conversation_id = %conversation_id,
                "routing to assistant.converse"
            );
            let model = ChatModelSelection {
                provider_id: options.provider_id.clone(),
                model_id: options.model_id.clone(),
            };
            assistant.converse(&conversation_id, input, &model).await
        } else {
            tracing::info!(
                phase = "send_model_message",
                mode = ?mode,
                conversation_id = %conversation_id,
                "routing to engine.chat"
            );
            engine
                .chat(input, conversation_id.clone(), options)
                .await
        };

        session_tracker.unregister(&conversation_id);

        let response = match result {
            Ok(response) => response,
            Err(error) => {
                tracing::error!(
                    phase = "send_model_message",
                    conversation_id = %conversation_id,
                    error_code = error.code(),
                    error = %error,
                    "send_model_message failed"
                );
                return Err(error);
            }
        };
        self.set_current_conversation_id(response.conversation_id.clone())?;
        Ok(response)
    }

    pub async fn assistant_step(
        &self,
        conversation_id: Option<String>,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        let conversation_id = self.resolve_existing_conversation_id(conversation_id)?;
        let conversation = self.store.require_conversation(&conversation_id)?;
        if conversation.mode != ConversationMode::Assistant {
            return Err(AppError::InvalidInput(
                "assistant step requires an Assistant session".into(),
            ));
        }
        let assistant = Arc::clone(&self.assistant);
        let session_tracker = self.session_tracker.clone();
        session_tracker.register(&conversation_id, None)?;
        let result = assistant.step(&conversation_id, model).await;
        session_tracker.unregister(&conversation_id);
        result
    }

    pub fn poll_status(&self) -> AppResult<PollerStatus> {
        Ok(self
            .poller
            .lock()
            .map_err(|e| AppError::StorageError(format!("Poller lock error: {e}")))?
            .status())
    }

    pub fn poll_pause(&self) -> AppResult<()> {
        self.poller
            .lock()
            .map_err(|e| AppError::StorageError(format!("Poller lock error: {e}")))?
            .pause();
        Ok(())
    }

    pub fn poll_resume(&self) -> AppResult<()> {
        self.poller
            .lock()
            .map_err(|e| AppError::StorageError(format!("Poller lock error: {e}")))?
            .resume();
        Ok(())
    }

    pub fn poll_trigger(&self) -> AppResult<()> {
        self.poller
            .lock()
            .map_err(|e| AppError::StorageError(format!("Poller lock error: {e}")))?
            .trigger();
        Ok(())
    }

    /// Manually trigger compaction for the current conversation.
    pub async fn compact_conversation(
        &self,
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

    pub fn clear_conversation(&self, conversation_id: Option<String>) -> AppResult<String> {
        let conversation_id = self.resolve_existing_conversation_id(conversation_id)?;
        self.store.clear_conversation(&conversation_id)?;

        let current = self.get_current_conversation_id()?;
        if current == conversation_id {
            let new_id = self
                .store
                .create_conversation(None, ConversationMode::Chat)?
                .id;
            self.set_current_conversation_id(new_id)?;
        }

        Ok(conversation_id)
    }

    /// Create a new blank conversation with the given mode and return its id.
    /// The current conversation is left unchanged.
    pub fn create_new_conversation(&self, mode: ConversationMode) -> AppResult<String> {
        let conv = self.store.create_conversation(None, mode)?;
        Ok(conv.id)
    }

    pub fn status(&self) -> AppResult<RuntimeStatus> {
        Ok(RuntimeStatus {
            app_name: "agent-app".to_string(),
            storage_path: self.store.root().display().to_string(),
            current_conversation_id: self.get_current_conversation_id()?,
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

    pub async fn bootstrap_neurons(&self) -> AppResult<()> {
        tracing::info!(phase = "bootstrap_neurons", "gateway bootstrap_neurons start");
        match self.neuron_manager.bootstrap().await {
            Ok(report) => {
                tracing::info!(
                    phase = "bootstrap_neurons",
                    create_neuron_id = %report.create_neuron_id,
                    select_neuron_id = %report.select_neuron_id,
                    "gateway bootstrap_neurons ok"
                );
                Ok(())
            }
            Err(error) => {
                tracing::warn!(
                    phase = "bootstrap_neurons",
                    error_code = error.code(),
                    error = %error,
                    "gateway bootstrap_neurons failed"
                );
                Err(error)
            }
        }
    }

    pub fn assistant(&self) -> Arc<AssistantMode> {
        Arc::clone(&self.assistant)
    }

    pub fn poller(&self) -> Arc<Mutex<Poller>> {
        Arc::clone(&self.poller)
    }

    pub fn providers(&self) -> ProviderRegistry {
        self.providers.clone()
    }

    pub fn conversation_store(&self) -> ConversationStore {
        self.store.clone()
    }

    /// Access the SessionTracker for TUI commands.
    pub fn session_tracker(&self) -> SessionTracker {
        self.session_tracker.clone()
    }

    fn get_current_conversation_id(&self) -> AppResult<String> {
        self.current_conversation_id
            .lock()
            .map(|guard| guard.clone())
            .map_err(|e| AppError::RuntimeError(format!("current_conversation_id lock: {e}")))
    }

    fn set_current_conversation_id(&self, id: String) -> AppResult<()> {
        let mut guard = self
            .current_conversation_id
            .lock()
            .map_err(|e| AppError::RuntimeError(format!("current_conversation_id lock: {e}")))?;
        *guard = id;
        Ok(())
    }

    fn resolve_conversation_id(&self, conversation_id: Option<String>) -> AppResult<String> {
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
            None => self.get_current_conversation_id(),
        }
    }

    fn resolve_existing_conversation_id(
        &self,
        conversation_id: Option<String>,
    ) -> AppResult<String> {
        let id = match conversation_id {
            Some(id) => id,
            None => self.get_current_conversation_id()?,
        };
        if id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Conversation id cannot be empty".into(),
            ));
        }

        self.store.require_conversation(&id)?;
        Ok(id)
    }
}

fn spawn_poller_runtime(
    poller: Arc<Mutex<Poller>>,
    assistant: Arc<AssistantMode>,
    providers: ProviderRegistry,
    mut step_rx: mpsc::UnboundedReceiver<AssistantStepRequest>,
    base_interval_ms: u64,
    state_emit: Option<StateEmitter>,
) {
    tracing::info!(
        phase = "poller_runtime",
        base_interval_ms,
        "poller runtime loop starting via tauri async runtime"
    );
    // 串行化 assistant step 处理：同一时刻只允许一个 PollAll 在跑，
    // 避免阻塞 tick 循环（select 分支内的 await 会拖住 interval），
    // 也避免并发推进同一批课题。
    let step_guard = Arc::new(tokio::sync::Mutex::new(()));
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(base_interval_ms));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Ok(mut guard) = poller.lock() {
                        guard.tick();
                        if let Some(emit) = state_emit.as_ref() {
                            emit(StateChange::Poller {
                                status: guard.status(),
                            });
                        }
                    }
                }
                Some(request) = step_rx.recv() => {
                    tracing::info!(phase = "poller_runtime", kind = "step_request", "received step request from channel");
                    let model = match providers.default_model_selection() {
                        Ok(Some(model)) => model,
                        _ => continue,
                    };
                    let assistant = assistant.clone();
                    let emit = state_emit.clone();
                    let step_guard = step_guard.clone();
                    // 放到独立任务执行，tick 循环立即返回，绝不被模型调用拖住。
                    tauri::async_runtime::spawn(async move {
                        let Ok(_permit) = step_guard.try_lock() else {
                            tracing::info!(phase = "poller_runtime", "step request skipped: another step is in flight");
                            return;
                        };
                        assistant.process_step_request(request, &model).await;
                        // 后台推进会写入会话/课题，通知前端重新拉取。
                        if let Some(emit) = emit.as_ref() {
                            emit(StateChange::Conversations);
                            emit(StateChange::Topics);
                        }
                    });
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn send_message_persists_user_and_assistant_messages() {
        let gateway = test_gateway("send_message_persists_user_and_assistant_messages");

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
    fn list_skills_includes_execute_command_tool() {
        let gateway = test_gateway("list_skills_includes_execute_command_tool");
        let skills = gateway.list_skills();
        assert!(
            skills.iter().any(|s| s.name == "execute_command"),
            "expected execute_command in tool registry, got: {:?}",
            skills
        );
    }

    #[test]
    fn clear_conversation_removes_selected_session() {
        let gateway = test_gateway("clear_conversation_removes_selected_session");
        let response = gateway
            .send_message("hello", None)
            .expect("message should be accepted");

        let cleared = gateway
            .clear_conversation(Some(response.conversation_id.clone()))
            .expect("conversation should clear");

        assert_eq!(cleared, response.conversation_id);
        assert!(gateway.history(Some(cleared)).is_err());
    }

    #[test]
    fn poller_status_available() {
        let gateway = test_gateway("poller_status_available");
        let status = gateway.poll_status().expect("poll status");
        assert_eq!(
            status.base_interval_ms,
            crate::core::poller::DEFAULT_POLLER_BASE_INTERVAL_MS
        );
        assert!(status.task_count >= 1);
        // Default / missing poller.enabled → paused
        assert_eq!(status.state, crate::core::poller::PollerRunState::Paused);
    }

    #[tokio::test]
    async fn send_model_message_rejects_missing_model_without_history_write() {
        let gateway =
            test_gateway("send_model_message_rejects_missing_model_without_history_write");
        let conversation_id = gateway
            .get_current_conversation_id()
            .expect("current conversation id");

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

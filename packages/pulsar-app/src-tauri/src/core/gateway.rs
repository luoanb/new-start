use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use tauri;
use tokio::sync::mpsc;

use super::{
    agent_session::AgentSession,
    assistant_session::AssistantSession,
    call_service::{ModelCaller, NeuronCallService, SessionSeed},
    chat_session::ChatSession,
    cmd_exec::ExecuteCommandTool,
    conversation_runner::ConversationRunner,
    compactor::Compactor,
    conversation_store::{now_ms, ConversationStore},
    current_time::GetCurrentTimeTool,
    dynamic_tool::{CommandTool, HttpTool},
    error::{AppError, AppResult},
    mcp::{McpServerClient, McpServerStatus, McpServerStatusKind},
    models::{
        ChatModelSelection, ChatOptions, ChatResponse, Conversation, ConversationMode, Message,
        MessageBody, MessageRole, ModelCallRequest, ModelCallResponse, ModelInfo, ProviderInfo,
        RuntimeStatus, SkillInfo, SystemPromptStatus, ToolInfo, ToolSource,
    },
    neuron_config::NeuronConfigReader,
    neuron_manager::NeuronManager,
    neuron_model::DefaultNeuronModelCaller,
    neuron_store::NeuronStore,
    poller::{new_shared_poll_parallelism, Poller, PollerConfigReader, PollerStatus},
    poller_step::AssistantStepRequest,
    providers::ProviderRegistry,
    session_tracker::SessionTracker,
    tool_config::{
        validate_tool_config, DynamicToolsFile, McpServersFile, ToolConfigReader, ToolConfigView,
    },
    tool_registry::ToolRegistry,
    topic_store::TopicStore,
    CompactionConfig,
};

use super::events::{StateChange, StateEmitter};

#[derive(Debug, Clone)]
pub struct Gateway {
    /// 手动压缩（/compact 命令）：Chat/Agent 会话过长时可显式触发；自动压缩已随
    /// `Engine` 退役（Chat = execute_round 退化形态，压缩由用户按需触发）。
    compactor: Compactor,
    store: ConversationStore,
    providers: ProviderRegistry,
    /// 共享工具注册表：启动期装配 + 运行期手动重装配（保存即生效）共用。
    /// 读锁只在 clone 结果/工具引用期间持有，不跨 await；重装配用写锁一次性替换。
    tool_registry: Arc<RwLock<ToolRegistry>>,
    topic_store: Option<Arc<Mutex<TopicStore>>>,
    neuron_store: Option<Arc<Mutex<NeuronStore>>>,
    neuron_manager: Arc<NeuronManager>,
    /// 业务接入（独立文件，业务逻辑不进入 Gateway 正文）。
    chat: ChatSession,
    agent: AgentSession,
    assistant: Arc<AssistantSession>,
    poller: Arc<Mutex<Poller>>,
    session_tracker: SessionTracker,
    /// Shared so Gateway can be used via `&self` / Tauri State without holding an outer lock across await.
    current_conversation_id: Arc<Mutex<String>>,
    /// MCP server 状态（装配期与运行期重装配均可更新，供前端 DockPane 展示）。
    mcp_server_statuses: Arc<RwLock<Vec<McpServerStatus>>>,
    /// 装配互斥：串行化启动期后台装配与运行期手动重装配，保证「以最后一次为准」。
    assemble_lock: Arc<tokio::sync::Mutex<()>>,
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
        Self::build(store, state_emit, None, None)
    }

    /// 测试专用构造：注入模型调用替身与工具注册表，使 Chat/Agent 收敛路径
    /// （execute_round / agent_loop）可在无真实 provider 环境下验证。
    #[cfg(test)]
    pub(crate) fn with_injected_for_test(
        store: ConversationStore,
        model_caller: Arc<dyn ModelCaller>,
        tool_registry: Arc<RwLock<ToolRegistry>>,
    ) -> AppResult<Self> {
        Self::build(store, None, Some(model_caller), Some(tool_registry))
    }

    /// 统一构造：`test_model_caller` / `test_tool_registry` 仅供测试注入。
    fn build(
        store: ConversationStore,
        state_emit: Option<StateEmitter>,
        test_model_caller: Option<Arc<dyn ModelCaller>>,
        test_tool_registry: Option<Arc<RwLock<ToolRegistry>>>,
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

        // 工具装配：本地通道（native + config）同步就绪、启动即可用；MCP 通道
        // 改为后台异步装配（不阻塞应用启动，连接完成自动登记并广播 Tools）。
        // 测试注入注册表时直接使用注入值（跳过本地/MCP 装配）。
        let tool_registry = match &test_tool_registry {
            Some(registry) => Arc::clone(registry),
            None => {
                let local_registry = assemble_local_tools(&store.root())?;
                Arc::new(RwLock::new(local_registry))
            }
        };
        let mcp_server_statuses: Arc<RwLock<Vec<McpServerStatus>>> =
            Arc::new(RwLock::new(Vec::new()));
        let assemble_lock: Arc<tokio::sync::Mutex<()>> = Arc::new(tokio::sync::Mutex::new(()));

        // 启动期后台装配 MCP：应用先起来（本地工具可用），MCP server 逐个连接，
        // 状态由 Connecting → Connected/Failed，前端经 StateChange::Tools 自动刷新。
        if let (Some(emit), None) = (state_emit.as_ref(), test_tool_registry.as_ref()) {
            let emit = Arc::clone(emit);
            let tool_registry = Arc::clone(&tool_registry);
            let mcp_server_statuses = Arc::clone(&mcp_server_statuses);
            let assemble_lock = Arc::clone(&assemble_lock);
            let storage_root = store.root().to_path_buf();
            tauri::async_runtime::spawn(async move {
                let _guard = assemble_lock.lock().await;
                let base_registry = match assemble_local_tools(&storage_root) {
                    Ok(registry) => registry,
                    Err(error) => {
                        tracing::error!(phase = "tool_config", error = %error, "background mcp assembly: local tools failed");
                        return;
                    }
                };
                if let Err(error) = assemble_mcp_progressive(
                    &storage_root,
                    base_registry,
                    &tool_registry,
                    &mcp_server_statuses,
                    Some(&emit),
                )
                .await
                {
                    tracing::error!(phase = "tool_config", error = %error, "background mcp assembly failed");
                }
            });
        }

        let neuron_config = NeuronConfigReader::new(store.root().to_path_buf());
        let neuron_recycle_interval_ms = neuron_config.recycle_interval_ms()?;
        let neuron_manager = Arc::new(NeuronManager::new(
            Arc::clone(&neuron_store),
            Arc::new(DefaultNeuronModelCaller::new(providers.clone())),
            neuron_config,
            Arc::clone(&tool_registry),
        ));
        let compactor = Compactor::new(CompactionConfig::default());

        let (step_tx, step_rx) = mpsc::unbounded_channel::<AssistantStepRequest>();

        let poller_settings = PollerConfigReader::new(store.root().to_path_buf()).load()?;
        tracing::info!(
            phase = "poller_config",
            enabled = poller_settings.enabled,
            base_interval_ms = poller_settings.base_interval_ms,
            assistant_interval_ticks = poller_settings.assistant_interval_ticks,
            assistant_poll_parallelism = poller_settings.assistant_poll_parallelism,
            "loaded poller settings from config"
        );
        // 并发数在 Poller（状态展示）与 AssistantMode（实际执行）之间共享同一原子值，
        // 前端调整后运行时立即生效并持久化到 config.json。
        let poll_parallelism =
            new_shared_poll_parallelism(poller_settings.assistant_poll_parallelism as usize);

        // 执行面：无状态单轮对话引擎（不持有 store；读会话/落库由 Runner + 业务文件负责）。
        let call_service = Arc::new(NeuronCallService::new(
            match test_model_caller {
                Some(caller) => caller,
                None => Arc::new(providers.clone()) as Arc<dyn ModelCaller>,
            },
            Arc::clone(&neuron_manager),
            Arc::clone(&tool_registry),
        ));

        // 单轮编排 + 业务接入（各业务独立文件，业务逻辑不进入 Gateway）。
        let runner = ConversationRunner::new(store.clone(), Arc::clone(&call_service));
        let chat = ChatSession::new(runner.clone());
        let agent = AgentSession::new(runner.clone(), Arc::clone(&tool_registry));
        let assistant = Arc::new(AssistantSession::new(
            store.clone(),
            Arc::clone(&neuron_manager),
            Arc::clone(&topic_store),
            Arc::clone(&neuron_store),
            runner.clone(),
            Arc::clone(&call_service),
            step_tx,
            session_tracker.clone(),
            Arc::clone(&poll_parallelism),
        ));

        let poller = Arc::new(Mutex::new(Poller::new(
            poller_settings.base_interval_ms,
            poll_parallelism,
        )));
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
            state_emit.clone(),
        );

        // 神经元容量回收：超限时后台定时逻辑删除最低价值节点，并通知前端刷新。
        spawn_neuron_recycle_runtime(
            Arc::clone(&neuron_manager),
            neuron_recycle_interval_ms,
            state_emit,
        );

        Ok(Self {
            compactor,
            store,
            providers,
            tool_registry,
            topic_store: Some(topic_store),
            neuron_store: Some(neuron_store),
            neuron_manager,
            chat,
            agent,
            assistant,
            poller,
            session_tracker,
            current_conversation_id: Arc::new(Mutex::new(current_conversation_id)),
            mcp_server_statuses,
            assemble_lock,
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
            body: MessageBody::Text {
                content: input.to_string(),
            },
            timestamp: now_ms(),
            neuron_id: None,
        };

        self.store.add_message(&conversation_id, user_message)?;
        let assistant_message = self.runtime_respond(input)?;
        let response = assistant_message.text().to_string();
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
            body: MessageBody::Text { content: response },
            timestamp: now_ms(),
            neuron_id: None,
        })
    }

    pub fn list_skills(&self) -> Vec<SkillInfo> {
        self.tool_registry
            .read()
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

    /// 开启会话：建会话 + 校验种子 + 写 `extra.session`（spec_neuron_id + seed + 空 state）。
    /// 种子元数据由命令层推导（spec_neuron_id → `Neuron(id)`，空 → `Global`）。
    pub fn start_session(
        &self,
        seed: Option<SessionSeed>,
        mode: ConversationMode,
    ) -> AppResult<Conversation> {
        if let Some(SessionSeed::Neuron(id)) = &seed {
            let neuron = self
                .neuron_manager
                .get(id)?
                .ok_or_else(|| AppError::NeuronNotFound(id.clone()))?;
            if neuron.system_type.is_some() && neuron.behavior.is_none() {
                return Err(AppError::InvalidInput(format!(
                    "spec neuron {id} is a system neuron without behavior"
                )));
            }
        }
        let mut conversation = self.store.create_conversation(None, mode.clone())?;
        let mut extra = conversation.extra.take().unwrap_or_else(|| serde_json::json!({}));
        if !extra.is_object() {
            extra = serde_json::json!({});
        }
        let mut session_meta = serde_json::json!({});
        if let Some(SessionSeed::Neuron(id)) = &seed {
            session_meta["spec_neuron_id"] = serde_json::json!(id);
        }
        session_meta["state"] = serde_json::json!({});
        if let Some(seed) = seed {
            session_meta["seed"] = serde_json::to_value(seed).unwrap_or_default();
        }
        let has_seed = session_meta.get("seed").is_some();
        extra["session"] = session_meta;
        conversation.extra = Some(extra);
        self.store.save_conversation(&conversation)?;
        tracing::info!(
            phase = "start_session",
            conversation_id = %conversation.id,
            mode = ?mode,
            has_seed,
            "session started"
        );
        Ok(conversation)
    }

    /// 列出所有 `session.%` 规格神经元（含 behavior 摘要，供前端「管理好后发起会话」）。
    pub fn list_session_specs(&self) -> AppResult<Vec<SystemPromptStatus>> {
        self.neuron_manager.list_session_specs()
    }

    /// 工具治理视图：全量工具（native / config / mcp）供前端 DockPane 展示。
    pub fn list_tool_info(&self) -> Vec<ToolInfo> {
        self.tool_registry
            .read()
            .map(|reg| {
                reg.list_definitions()
                    .into_iter()
                    .map(|d| ToolInfo {
                        name: d.name,
                        description: d.description,
                        source: d.source,
                        parameters: d.parameters,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// MCP server 连接状态（装配期与重装配后均可读取）。
    pub fn mcp_server_statuses(&self) -> Vec<McpServerStatus> {
        self.mcp_server_statuses
            .read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// 读取当前工具配置（供前端弹窗编辑）。
    pub fn get_tool_config(&self) -> AppResult<ToolConfigView> {
        let reader = ToolConfigReader::new(self.store.root().to_path_buf());
        let mcp = reader.mcp_servers()?;
        let dynamic = reader.dynamic_tools()?;
        Ok(ToolConfigView {
            mcp_servers: mcp.mcp_servers,
            http_tools: dynamic.http,
            command_tools: dynamic.command,
        })
    }

    /// 保存工具配置：校验 → 原子写回 JSON → 全量重建 registry → 替换运行期引用。
    /// 保存即生效，无需重启。非法配置在写文件前被拒绝。
    pub async fn save_tool_config(&self, view: ToolConfigView) -> AppResult<ToolConfigView> {
        validate_tool_config(&view)?;
        let reader = ToolConfigReader::new(self.store.root().to_path_buf());
        reader.save_mcp_servers(&McpServersFile {
            mcp_servers: view.mcp_servers.clone(),
        })?;
        reader.save_dynamic_tools(&DynamicToolsFile {
            http: view.http_tools.clone(),
            command: view.command_tools.clone(),
        })?;

        self.assemble_and_replace().await?;

        tracing::info!(
            phase = "tool_config",
            mcp_servers = view.mcp_servers.len(),
            http_tools = view.http_tools.len(),
            command_tools = view.command_tools.len(),
            "tool config saved and registry reassembled"
        );

        self.get_tool_config()
    }

    /// 重新装配：读取磁盘上的 `mcp_servers.json` / `dynamic_tools.json` 并全量重建
    /// 工具集（不写文件）。供前端「刷新」按钮使用——配置文件在外部被修改后，
    /// 无需打开弹窗即可让变更生效。配置非法时返回可读错误，registry 保持原状。
    pub async fn reassemble_tools(&self) -> AppResult<()> {
        self.assemble_and_replace().await?;
        tracing::info!(phase = "tool_config", "tool registry reassembled from disk");
        Ok(())
    }

    /// 全量重装配并原子替换共享状态。
    ///
    /// 真正的 async 装配（无 `block_on`）：本地通道（native + config）同步装配，
    /// MCP 通道逐 server 连接 + `tools/list`（各自超时），失败 warn + skip。
    /// 通过装配互斥与启动期后台装配串行化，保证「以最后一次为准」。
    async fn assemble_and_replace(&self) -> AppResult<()> {
        let _guard = self.assemble_lock.lock().await;
        let base_registry = assemble_local_tools(&self.store.root())?;
        let (new_registry, new_statuses) = assemble_mcp_progressive(
            &self.store.root(),
            base_registry,
            &self.tool_registry,
            &self.mcp_server_statuses,
            None,
        )
        .await?;
        self.replace_tools(new_registry, new_statuses)
    }

    /// 原子替换共享 registry 与 MCP 状态（在飞工具调用持有旧 Arc 引用，不受影响）。
    fn replace_tools(
        &self,
        new_registry: ToolRegistry,
        new_statuses: Vec<McpServerStatus>,
    ) -> AppResult<()> {
        replace_tools_shared(&self.tool_registry, new_registry)?;
        replace_statuses_shared(&self.mcp_server_statuses, new_statuses)
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
        let model = ChatModelSelection {
            provider_id: options.provider_id.clone(),
            model_id: options.model_id.clone(),
        };

        // Clone handles before any network await — callers must not hold an outer Gateway lock.
        let assistant = Arc::clone(&self.assistant);
        let session_tracker = self.session_tracker.clone();
        session_tracker.register(&conversation_id, None)?;

        // ConversationMode 路由按 mode 委托各业务 session 文件（业务逻辑不进 Gateway）：
        // - Assistant → assistant_session.converse（课题 hooks 编排在 AssistantHooks）
        // - Chat     → chat_session.send（直连，无选型/无工具）
        // - Agent    → agent_session.agent_loop（全工具多轮循环 + 护栏）
        let result = match mode {
            ConversationMode::Assistant => {
                tracing::info!(
                    phase = "send_model_message",
                    mode = "assistant",
                    conversation_id = %conversation_id,
                    "routing to assistant_session.converse"
                );
                assistant.converse(&conversation_id, input, &model).await
            }
            ConversationMode::Chat => {
                tracing::info!(
                    phase = "send_model_message",
                    mode = "chat",
                    conversation_id = %conversation_id,
                    "routing to chat_session.send"
                );
                self.chat.send(&conversation_id, input, &model).await
            }
            ConversationMode::Agent => {
                tracing::info!(
                    phase = "send_model_message",
                    mode = "agent",
                    conversation_id = %conversation_id,
                    "routing to agent_session.agent_loop"
                );
                self.agent.agent_loop(&conversation_id, input, &model).await
            }
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

    /// 设置轮询并发推进数量：持久化到 config.json（统一 ConfigStore 接口），
    /// 并更新共享运行时值（Poller 与 AssistantMode 同时生效）。
    pub fn set_poll_parallelism(&self, n: u64) -> AppResult<u64> {
        let clamped = n.clamp(1, super::poller::MAX_ASSISTANT_POLL_PARALLELISM);
        PollerConfigReader::new(self.store.root().to_path_buf()).set_parallelism(clamped)?;
        let guard = self
            .poller
            .lock()
            .map_err(|e| AppError::StorageError(format!("Poller lock error: {e}")))?;
        guard.set_parallelism(clamped as usize);
        Ok(clamped)
    }

    /// Manually trigger compaction for the current conversation.
    pub async fn compact_conversation(&self, conversation_id: Option<String>) -> AppResult<String> {
        let id = self.resolve_existing_conversation_id(conversation_id)?;
        let model = self
            .default_model_selection()?
            .ok_or(AppError::ModelNotSelected)?;
        let mut conversation = self.store.require_conversation(&id)?;
        let compacted = self
            .compactor
            .compact(&mut conversation, &self.providers, &model)
            .await?;
        if compacted {
            self.store.save_conversation(&conversation)?;
        }
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
            app_name: "pulsar".to_string(),
            storage_path: self.store.root().display().to_string(),
            current_conversation_id: self.get_current_conversation_id()?,
            skill_count: self
                .tool_registry
                .read()
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
        tracing::info!(
            phase = "bootstrap_neurons",
            "gateway bootstrap_neurons start"
        );
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

    pub fn assistant(&self) -> Arc<AssistantSession> {
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
    assistant: Arc<AssistantSession>,
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
                        let touched = assistant.process_step_request(request, &model).await;
                        // 仅在实际推进了会话（写入消息/课题）时才通知前端重新拉取；
                        // 空转轮询（无未完成课题 / 全部跳过）不发事件，避免无效刷新与滚动。
                        if !touched.is_empty() {
                            if let Some(emit) = emit.as_ref() {
                                emit(StateChange::Conversations { affected: touched });
                                emit(StateChange::Topics);
                            }
                        }
                    });
                }
            }
        }
    });
}

/// 定时回收低价值神经元：周期由 config `neuron.recycle_interval_ms` 控制，
/// 超容量时按低价值排序逻辑删除，并通知前端刷新神经元面板。
fn spawn_neuron_recycle_runtime(
    neurons: Arc<NeuronManager>,
    interval_ms: u64,
    state_emit: Option<StateEmitter>,
) {
    tracing::info!(
        phase = "neuron_recycle_runtime",
        interval_ms,
        "neuron recycle runtime loop starting via tauri async runtime"
    );
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
        loop {
            interval.tick().await;
            match neurons.recycle_if_over_capacity() {
                Ok(recycled) if recycled > 0 => {
                    tracing::info!(
                        phase = "neuron_recycle_runtime",
                        recycled,
                        "recycled low-value neurons over capacity"
                    );
                    if let Some(emit) = state_emit.as_ref() {
                        emit(StateChange::Neurons);
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(
                        phase = "neuron_recycle_runtime",
                        error_code = error.code(),
                        error = %error,
                        "neuron recycle check failed"
                    );
                }
            }
        }
    });
}

/// 本地通道装配（native + config，同步、无网络）：启动即可用。
///
/// `execute_command` 是首个上架 native 工具（见 inserts/execute_command.md），
/// `get_current_time` 为第二个（见 inserts/get_current_time.md）。
/// 配置驱动通道：dynamic_tools.json（HttpTool / CommandTool）。声明即 schema，
/// 豁免 insert 门禁；命令模板复用 cmd_exec 安全护栏。
fn assemble_local_tools(storage_root: &std::path::Path) -> AppResult<ToolRegistry> {
    let mut registry = ToolRegistry::new();
    registry.register(ExecuteCommandTool::new());
    registry.register(GetCurrentTimeTool::new());
    let tool_config = ToolConfigReader::new(storage_root.to_path_buf());
    let dynamic = tool_config.dynamic_tools()?;
    for cfg in dynamic.http {
        registry.register_source(HttpTool::from_config(&cfg), ToolSource::Config);
    }
    for cfg in dynamic.command {
        registry.register_source(CommandTool::from_config(&cfg), ToolSource::Config);
    }
    Ok(registry)
}

/// 逐 server 装配 MCP 工具并渐进替换共享 registry / statuses。
///
/// 启动（后台 spawn）与运行期（`save_tool_config` / `reassemble_tools`）共用，
/// 真正的 async 装配（无 `block_on`）：连接与 `tools/list` 各自有超时，
/// 单个 server 失败仅记录 Failed 并 warn + skip，不阻塞整体。
///
/// 流程：
/// - 以 `base_registry`（纯本地工具）为起点，先替换共享 registry，清掉旧 MCP 工具；
/// - statuses 初始占位：启用 server 记 Connecting（前端「连接中」），disabled 记 Disabled；
/// - 每 server 完成：并入其工具、更新状态，原子替换两个共享状态，若提供 emit 则广播 Tools；
/// - 返回最终 registry + statuses。
///
/// 配置解析失败向上传播（用户声明错误），不替换任何共享状态。
async fn assemble_mcp_progressive(
    storage_root: &std::path::Path,
    base_registry: ToolRegistry,
    tool_registry: &Arc<RwLock<ToolRegistry>>,
    mcp_server_statuses: &Arc<RwLock<Vec<McpServerStatus>>>,
    emit: Option<&StateEmitter>,
) -> AppResult<(ToolRegistry, Vec<McpServerStatus>)> {
    let reader = ToolConfigReader::new(storage_root.to_path_buf());
    let servers = reader.mcp_servers()?;

    let mut registry = base_registry;
    let mut statuses = Vec::with_capacity(servers.mcp_servers.len());
    for cfg in &servers.mcp_servers {
        statuses.push(McpServerStatus {
            name: cfg.name.clone(),
            transport: cfg.transport.clone(),
            status: if cfg.disabled {
                McpServerStatusKind::Disabled
            } else {
                McpServerStatusKind::Connecting
            },
            tool_count: 0,
            error: None,
        });
    }

    // 初始占位：registry = 纯本地；启用 server 显示「连接中」并广播。
    replace_tools_shared(tool_registry, registry.clone())?;
    replace_statuses_shared(mcp_server_statuses, statuses.clone())?;
    if let Some(emit) = emit {
        emit(StateChange::Tools);
    }

    for (idx, cfg) in servers.mcp_servers.iter().enumerate() {
        if cfg.disabled {
            continue;
        }
        let name = cfg.name.clone();
        let transport = cfg.transport.clone();
        let status = match McpServerClient::connect(cfg.clone()).await {
            Ok(client) => match client.discover_tools().await {
                Ok(tools) => {
                    let tool_count = tools.len();
                    if tool_count == 0 {
                        tracing::warn!(server = %name, "mcp server advertised no tools");
                    }
                    for tool in tools {
                        registry.register_source(tool, ToolSource::Mcp);
                    }
                    McpServerStatus {
                        name,
                        transport,
                        status: McpServerStatusKind::Connected,
                        tool_count,
                        error: None,
                    }
                }
                Err(error) => {
                    tracing::warn!(server = %name, error = %error, "mcp server tools/list failed; skipped");
                    McpServerStatus {
                        name,
                        transport,
                        status: McpServerStatusKind::Failed,
                        tool_count: 0,
                        error: Some(error.to_string()),
                    }
                }
            },
            Err(error) => {
                tracing::warn!(server = %name, error = %error, "mcp server connect failed; skipped");
                McpServerStatus {
                    name,
                    transport,
                    status: McpServerStatusKind::Failed,
                    tool_count: 0,
                    error: Some(error.to_string()),
                }
            }
        };
        statuses[idx] = status;
        replace_tools_shared(tool_registry, registry.clone())?;
        replace_statuses_shared(mcp_server_statuses, statuses.clone())?;
        if let Some(emit) = emit {
            emit(StateChange::Tools);
        }
    }
    Ok((registry, statuses))
}

/// 原子替换共享 tool registry（在飞工具调用持有旧 Arc 引用，不受影响）。
fn replace_tools_shared(
    tool_registry: &Arc<RwLock<ToolRegistry>>,
    new_registry: ToolRegistry,
) -> AppResult<()> {
    let mut reg = tool_registry
        .write()
        .map_err(|e| AppError::RuntimeError(format!("tool registry lock: {e}")))?;
    *reg = new_registry;
    Ok(())
}

/// 原子替换共享 MCP server 状态。
fn replace_statuses_shared(
    mcp_server_statuses: &Arc<RwLock<Vec<McpServerStatus>>>,
    new_statuses: Vec<McpServerStatus>,
) -> AppResult<()> {
    let mut status = mcp_server_statuses
        .write()
        .map_err(|e| AppError::RuntimeError(format!("mcp status lock: {e}")))?;
    *status = new_statuses;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        call_service::ModelCaller,
        models::ToolCall,
        tool_config::{HttpToolConfig, McpServerConfig},
        tool_registry::Tool,
    };
    use async_trait::async_trait;
    use std::{fs, path::PathBuf, sync::Mutex as StdMutex};

    /// 与 `agent_session::AGENT_MAX_ITERATIONS` 对齐的测试护栏常量。
    const AGENT_MAX_ITERATIONS: u32 = 20;

    /// Agent 循环测试替身：可编程响应序列（顺序消耗，耗尽后报错）。
    struct ScriptedModelCaller {
        responses: Arc<StdMutex<Vec<ModelCallResponse>>>,
    }

    #[async_trait]
    impl ModelCaller for ScriptedModelCaller {
        async fn call_model(&self, _request: ModelCallRequest) -> AppResult<ModelCallResponse> {
            let mut guard = self
                .responses
                .lock()
                .map_err(|e| AppError::RuntimeError(format!("scripted caller lock: {e}")))?;
            if guard.is_empty() {
                return Err(AppError::RuntimeError(
                    "scripted model caller exhausted".into(),
                ));
            }
            Ok(guard.remove(0))
        }
    }

    struct EchoTool;
    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "echo tool"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(&self, args: serde_json::Value) -> AppResult<String> {
            Ok(format!("echo:{args}"))
        }
    }

    fn tool_call_response() -> ModelCallResponse {
        ModelCallResponse {
            provider_id: "fake".into(),
            model_id: "fake".into(),
            output: "calling echo".into(),
            tool_calls: Some(vec![ToolCall {
                id: "call_1".into(),
                name: "echo".into(),
                arguments: serde_json::json!({"text": "hi"}),
            }]),
            finish_reason: "tool_calls".into(),
        }
    }

    #[tokio::test]
    async fn agent_loop_converges_after_tool_round() {
        let store = ConversationStore::new(test_root("agent_loop_converges_after_tool_round"))
            .expect("test store should initialize");
        let mut registry = ToolRegistry::new();
        registry.register_source(EchoTool, ToolSource::Config);
        let caller: Arc<dyn ModelCaller> = Arc::new(ScriptedModelCaller {
            responses: Arc::new(StdMutex::new(vec![
                tool_call_response(),
                ModelCallResponse {
                    provider_id: "fake".into(),
                    model_id: "fake".into(),
                    output: "task done".into(),
                    tool_calls: None,
                    finish_reason: "stop".into(),
                },
            ])),
        });
        let gateway =
            Gateway::with_injected_for_test(store, caller, Arc::new(RwLock::new(registry)))
                .expect("test gateway should initialize");
        let conv = gateway
            .store
            .create_conversation(None, ConversationMode::Agent)
            .expect("agent conversation should be created");
        let model = ChatModelSelection {
            provider_id: "fake".into(),
            model_id: "fake".into(),
        };

        let response = gateway
            .agent
            .agent_loop(&conv.id, "do it", &model)
            .await
            .expect("agent loop should converge");

        assert_eq!(response.response, "task done");
        // 历史：user + assistant(tool_call) + tool(result) + assistant(text)。
        let history = gateway.history(Some(conv.id)).expect("history should load");
        assert_eq!(history.len(), 4);
        assert_eq!(history[0].role, MessageRole::User);
        assert!(matches!(history[1].body, MessageBody::ToolCall { .. }));
        assert!(matches!(history[2].body, MessageBody::ToolResult { .. }));
        assert_eq!(history[3].role, MessageRole::Assistant);
        assert_eq!(history[3].text(), "task done");
    }

    #[tokio::test]
    async fn agent_loop_hits_max_iterations_guard() {
        let store = ConversationStore::new(test_root("agent_loop_hits_max_iterations_guard"))
            .expect("test store should initialize");
        let mut registry = ToolRegistry::new();
        registry.register_source(EchoTool, ToolSource::Config);
        // 固定返回 tool_calls：20 轮工具执行后触发 AGENT_MAX_ITERATIONS 护栏。
        let caller: Arc<dyn ModelCaller> = Arc::new(ScriptedModelCaller {
            responses: Arc::new(StdMutex::new(
                (0..AGENT_MAX_ITERATIONS)
                    .map(|_| tool_call_response())
                    .collect(),
            )),
        });
        let gateway =
            Gateway::with_injected_for_test(store, caller, Arc::new(RwLock::new(registry)))
                .expect("test gateway should initialize");
        let conv = gateway
            .store
            .create_conversation(None, ConversationMode::Agent)
            .expect("agent conversation should be created");
        let model = ChatModelSelection {
            provider_id: "fake".into(),
            model_id: "fake".into(),
        };

        let err = gateway
            .agent
            .agent_loop(&conv.id, "loop", &model)
            .await
            .expect_err("agent loop should exceed max iterations");
        assert_eq!(err.code(), "agent_max_iterations");
    }

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
    fn list_skills_includes_get_current_time_tool() {
        let gateway = test_gateway("list_skills_includes_get_current_time_tool");
        let skills = gateway.list_skills();
        assert!(
            skills.iter().any(|s| s.name == "get_current_time"),
            "expected get_current_time in tool registry, got: {:?}",
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

    #[tokio::test]
    async fn save_tool_config_reassembles_registry() {
        let gateway = test_gateway("save_tool_config_reassembles_registry");
        let mut view = gateway.get_tool_config().expect("empty config");
        view.http_tools.push(HttpToolConfig {
            name: "lookup_wiki".into(),
            desc: "查内部 wiki".into(),
            method: "GET".into(),
            url: "https://api.example.com/wiki?q={query}".into(),
            timeout_ms: None,
        });

        let saved = gateway
            .save_tool_config(view)
            .await
            .expect("save should succeed");
        assert_eq!(saved.http_tools.len(), 1);
        assert_eq!(saved.http_tools[0].name, "lookup_wiki");

        // 保存即生效：registry 已重建并包含新工具，无需重启。
        let tools = gateway.list_tool_info();
        let tool = tools
            .iter()
            .find(|t| t.name == "lookup_wiki")
            .expect("lookup_wiki should be registered after reassembly");
        assert_eq!(tool.source, ToolSource::Config);
    }

    #[tokio::test]
    async fn save_tool_config_rejects_invalid_and_keeps_previous() {
        let gateway = test_gateway("save_tool_config_rejects_invalid_and_keeps_previous");
        let mut view = gateway.get_tool_config().expect("empty config");
        view.mcp_servers.push(McpServerConfig {
            name: "broken".into(),
            transport: "sse".into(), // 非法 transport
            command: None,
            args: vec![],
            env: Default::default(),
            url: None,
            headers: Default::default(),
            disabled: false,
        });

        let err = gateway
            .save_tool_config(view)
            .await
            .expect_err("invalid transport should be rejected");
        assert!(err.to_string().contains("未知 transport"));
        // 拒绝保存：registry 与状态保持原状。
        assert!(gateway.mcp_server_statuses().is_empty());
        assert!(!gateway.list_tool_info().iter().any(|t| t.name == "broken"));
    }

    #[test]
    fn get_tool_config_returns_default_empty_view() {
        let gateway = test_gateway("get_tool_config_returns_default_empty_view");
        let view = gateway.get_tool_config().expect("empty default view");
        assert!(view.mcp_servers.is_empty());
        assert!(view.http_tools.is_empty());
        assert!(view.command_tools.is_empty());
    }

    #[tokio::test]
    async fn reassemble_tools_loads_disk_config() {
        let root = test_root("reassemble_tools_loads_disk_config");
        if root.exists() {
            fs::remove_dir_all(&root).expect("old test storage should be removable");
        }
        let store = ConversationStore::new(root.clone()).expect("test store should initialize");
        let gateway = Gateway::new(store).expect("test gateway should initialize");
        assert!(!gateway
            .list_tool_info()
            .iter()
            .any(|t| t.name == "lookup_wiki"));

        // 模拟外部修改配置：直接写 dynamic_tools.json。
        ToolConfigReader::new(&root)
            .save_dynamic_tools(&DynamicToolsFile {
                http: vec![HttpToolConfig {
                    name: "lookup_wiki".into(),
                    desc: "查内部 wiki".into(),
                    method: "GET".into(),
                    url: "https://api.example.com/wiki?q={query}".into(),
                    timeout_ms: None,
                }],
                command: vec![],
            })
            .expect("write config");

        gateway
            .reassemble_tools()
            .await
            .expect("reassemble should succeed");
        let tools = gateway.list_tool_info();
        assert!(
            tools.iter().any(|t| t.name == "lookup_wiki"),
            "expected lookup_wiki after reassemble: {:?}",
            tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>()
        );
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
        std::env::temp_dir().join(format!("pulsar-{name}-{}", now_ms()))
    }
}

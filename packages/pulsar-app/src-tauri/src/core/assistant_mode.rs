use std::fmt;
use std::sync::{atomic::Ordering, Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;

use super::{
    call_service::{
        message_to_model, read_session_state, write_session_state, AssistantRoundContext,
        NeuronCallService, RoundTrigger,
    },
    conversation_store::{now_ms, ConversationStore},
    error::{AppError, AppResult},
    model_call_input::ModelCallInput,
    models::{ChatModelSelection, ChatResponse, ScopeInItem, Topic, TopicStatus, TopicUpdate},
    neuron_manager::NeuronManager,
    neuron_store::NeuronStore,
    poller::{PollHandler, Poller, SharedPollParallelism},
    providers::ProviderRegistry,
    topic_store::TopicStore,
};

pub const SYSTEM_TYPE_SELECT_NEURON: &str = "assistant_select_neuron";
pub const SYSTEM_TYPE_MATCH_TOPIC: &str = "assistant_match_topic";
pub const SYSTEM_TYPE_COMPLETE_SCOPE: &str = "assistant_complete_scope";
pub const SYSTEM_TYPE_SCORE_FEEDBACK: &str = "assistant_score_feedback";
pub const ASSISTANT_POLL_TASK: &str = "assistant_advance";

/// Re-export default interval ticks (overridable via `config.json` → `poller`).
pub use super::poller::DEFAULT_ASSISTANT_POLL_TICKS;

#[async_trait]
pub trait BeforeHook: Send + Sync {
    async fn run(&self, ctx: &mut AssistantRoundContext) -> AppResult<()>;
}

#[async_trait]
pub trait AfterHook: Send + Sync {
    async fn run(&self, ctx: &mut AssistantRoundContext) -> AppResult<()>;
}

#[derive(Debug, Clone)]
pub enum AssistantStepRequest {
    PollAll,
}

#[derive(Clone)]
pub struct AssistantMode {
    store: ConversationStore,
    providers: ProviderRegistry,
    neuron_manager: Arc<NeuronManager>,
    topic_store: Arc<Mutex<TopicStore>>,
    neuron_store: Arc<Mutex<NeuronStore>>,
    /// 执行面：规格解析 / 选型 / 模型调用 / 会话态（收敛 SelectNeuronBeforeHook + authorize_tools + run_core）。
    call_service: Arc<NeuronCallService>,
    step_tx: UnboundedSender<AssistantStepRequest>,
    session_tracker: super::session_tracker::SessionTracker,
    /// 与 Poller 共享的轮询并发推进数量（运行时可变，前端可调）。
    poll_parallelism: SharedPollParallelism,
}

impl fmt::Debug for AssistantMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssistantMode").finish_non_exhaustive()
    }
}

impl AssistantMode {
    pub fn new(
        store: ConversationStore,
        providers: ProviderRegistry,
        neuron_manager: Arc<NeuronManager>,
        topic_store: Arc<Mutex<TopicStore>>,
        neuron_store: Arc<Mutex<NeuronStore>>,
        call_service: Arc<NeuronCallService>,
        step_tx: UnboundedSender<AssistantStepRequest>,
        session_tracker: super::session_tracker::SessionTracker,
        poll_parallelism: SharedPollParallelism,
    ) -> Self {
        Self {
            store,
            providers,
            neuron_manager,
            topic_store,
            neuron_store,
            call_service,
            step_tx,
            session_tracker,
            poll_parallelism,
        }
    }

    /// 更新轮询并发推进数量（运行时生效），返回实际生效值。
    pub fn set_poll_parallelism(&self, n: usize) -> usize {
        let n = n.max(1);
        self.poll_parallelism.store(n, Ordering::Relaxed);
        n
    }

    pub fn enqueue_poll_all(&self) {
        let _ = self.step_tx.send(AssistantStepRequest::PollAll);
    }

    pub async fn converse(
        &self,
        session_id: &str,
        user_input: &str,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        tracing::info!(
            phase = "assistant_converse",
            session_id,
            provider = %model.provider_id,
            model = %model.model_id,
            input_len = user_input.len(),
            "converse start"
        );
        let mut ctx = self
            .build_context(session_id, RoundTrigger::UserInput)
            .await?;
        ctx.user_input = Some(user_input.to_string());
        tracing::info!(
            phase = "assistant_converse",
            session_id,
            topic_id = ctx.topic_id.as_deref().unwrap_or(""),
            "context built"
        );

        tracing::info!(
            phase = "assistant_converse",
            step = "score_feedback",
            "beforehook start"
        );
        if let Err(error) = (ScoreFeedbackBeforeHook { assistant: self })
            .run(&mut ctx)
            .await
        {
            tracing::error!(
                phase = "assistant_converse",
                step = "score_feedback",
                error_code = error.code(),
                error = %error,
                "beforehook failed"
            );
            return Err(error);
        }
        tracing::info!(
            phase = "assistant_converse",
            step = "score_feedback",
            "beforehook ok"
        );

        tracing::info!(
            phase = "assistant_converse",
            step = "match_topic",
            "beforehook start"
        );
        if let Err(error) = (MatchTopicBeforeHook { assistant: self })
            .run(&mut ctx)
            .await
        {
            tracing::error!(
                phase = "assistant_converse",
                step = "match_topic",
                error_code = error.code(),
                error = %error,
                "beforehook failed"
            );
            return Err(error);
        }
        tracing::info!(
            phase = "assistant_converse",
            step = "match_topic",
            session_id = %ctx.session_id,
            topic_id = ctx.topic_id.as_deref().unwrap_or(""),
            "match_topic ok"
        );
        tracing::info!(
            phase = "assistant_converse",
            step = "resolve_round",
            "spec resolve start"
        );
        if let Err(error) = self.call_service.resolve_round(&mut ctx).await {
            tracing::error!(
                phase = "assistant_converse",
                step = "resolve_round",
                error_code = error.code(),
                error = %error,
                "resolve_round failed"
            );
            return Err(error);
        }
        tracing::info!(
            phase = "assistant_converse",
            step = "execute_round",
            neuron_id = ctx
                .selected_neuron
                .as_ref()
                .map(|n| n.id.as_str())
                .unwrap_or(""),
            tools = ctx.authorized_tool_ids.len(),
            "entering execute_round"
        );
        let response = match self.call_service.execute_round(&mut ctx, model).await {
            Ok(response) => response,
            Err(error) => {
                tracing::error!(
                    phase = "assistant_converse",
                    step = "execute_round",
                    error_code = error.code(),
                    error = %error,
                    "execute_round failed"
                );
                return Err(error);
            }
        };
        if let Err(error) = (CompleteScopeAfterHook { assistant: self })
            .run(&mut ctx)
            .await
        {
            tracing::error!(
                phase = "assistant_converse",
                step = "complete_scope",
                error_code = error.code(),
                error = %error,
                "afterhook failed"
            );
            eprintln!("assistant afterhook failed: {error}");
            return Err(error);
        }
        self.mark_user_intervention(&ctx)?;
        tracing::info!(
            phase = "assistant_converse",
            session_id = %response.conversation_id,
            response_len = response.response.len(),
            "converse ok"
        );
        Ok(response)
    }

    pub async fn step(
        &self,
        session_id: &str,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        tracing::info!(phase = "assistant_step", session_id, "step start");
        let mut ctx = self
            .build_context(session_id, RoundTrigger::ManualStep)
            .await?;
        if ctx.topic_id.is_none() {
            return Err(AppError::InvalidInput(
                "Assistant step requires a topic bound to the session".into(),
            ));
        }
        self.call_service.resolve_round(&mut ctx).await?;
        let response = self.call_service.execute_round(&mut ctx, model).await?;
        if let Err(error) = (CompleteScopeAfterHook { assistant: self })
            .run(&mut ctx)
            .await
        {
            tracing::error!(
                phase = "assistant_step",
                step = "complete_scope",
                error = %error,
                "afterhook failed"
            );
            eprintln!("assistant afterhook failed: {error}");
            return Err(error);
        }
        self.bump_poll_count(&ctx)?;
        tracing::info!(phase = "assistant_step", session_id, "step ok");
        Ok(response)
    }

    pub async fn step_poller(
        &self,
        session_id: &str,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        tracing::info!(phase = "assistant_poller", session_id, "poller step start");
        let mut ctx = self.build_context(session_id, RoundTrigger::Poller).await?;
        if ctx.topic_id.is_none() {
            return Err(AppError::InvalidInput(
                "Assistant poller step requires a topic bound to the session".into(),
            ));
        }
        if let Err(error) = self.call_service.resolve_round(&mut ctx).await {
            tracing::error!(
                phase = "assistant_poller",
                step = "resolve_round",
                error = %error,
                "resolve failed"
            );
            eprintln!("assistant poller resolve failed: {error}");
            return Err(error);
        }
        let response = match self.call_service.execute_round(&mut ctx, model).await {
            Ok(response) => response,
            Err(error) => {
                tracing::error!(
                    phase = "assistant_poller",
                    step = "execute_round",
                    error = %error,
                    "core failed"
                );
                eprintln!("assistant poller core failed: {error}");
                return Err(error);
            }
        };
        if let Err(error) = (CompleteScopeAfterHook { assistant: self })
            .run(&mut ctx)
            .await
        {
            eprintln!("assistant poller afterhook failed: {error}");
        }
        let _ = self.bump_poll_count(&ctx);
        Ok(response)
    }

    pub fn register_polling(&self, poller: &mut Poller, interval_ticks: u64) -> AppResult<()> {
        let tx = self.step_tx.clone();
        poller.register(
            ASSISTANT_POLL_TASK,
            interval_ticks.max(1),
            Box::new(AssistantPollHandler { tx }),
        )
    }

    pub async fn process_step_request(
        self: Arc<Self>,
        request: AssistantStepRequest,
        model: &ChatModelSelection,
    ) {
        match request {
            AssistantStepRequest::PollAll => {
                tracing::info!(
                    phase = "assistant_poll_handler",
                    "PollAll received in process_step_request"
                );
                let topics = match self.topics().and_then(|store| store.list_unfinished()) {
                    Ok(topics) => topics,
                    Err(error) => {
                        eprintln!("assistant poll list failed: {error}");
                        return;
                    }
                };
                tracing::info!(
                    phase = "assistant_poll_handler",
                    topic_count = topics.len(),
                    "PollAll topic list resolved"
                );

                // 跨课题受限并发推进：每个课题绑定唯一会话、互不干扰，
                // 同一时刻最多“当前配置的并发数”个课题在跑；全局 step_guard 仍保证
                // 同一时刻只有一个 PollAll 在推进（超时周期被丢弃而非并发）。
                let parallelism = self.poll_parallelism.load(Ordering::Relaxed).max(1);
                let semaphore = Arc::new(tokio::sync::Semaphore::new(parallelism));
                let mut tasks = tokio::task::JoinSet::new();
                for topic in topics {
                    let Some(session_id) = topic.session_id else {
                        continue;
                    };
                    if matches!(topic.status, TopicStatus::Paused | TopicStatus::Cancelled) {
                        continue;
                    }
                    let topic_id = topic.id;
                    let model = model.clone();
                    let assistant = Arc::clone(&self);
                    let semaphore = Arc::clone(&semaphore);
                    tasks.spawn(async move {
                        let _permit = semaphore.acquire().await.expect("semaphore not closed");
                        if let Err(error) = assistant.session_tracker.register(&session_id, None) {
                            eprintln!("assistant poll register failed for {topic_id}: {error}");
                            return;
                        }
                        let _ = assistant
                            .session_tracker
                            .update_step(&session_id, "polling");
                        if let Err(error) = assistant.step_poller(&session_id, &model).await {
                            eprintln!("assistant poll step failed for {topic_id}: {error}");
                        }
                        assistant.session_tracker.unregister(&session_id);
                    });
                }
                while tasks.join_next().await.is_some() {}
            }
        }
    }

    async fn build_context(
        &self,
        session_id: &str,
        trigger: RoundTrigger,
    ) -> AppResult<AssistantRoundContext> {
        let conversation = self.store.require_conversation(session_id)?;
        let topic = self.topics()?.find_by_session_id(session_id)?;
        // poll_count 仍留 topic.extra.assistant。
        let poll_count = topic
            .as_ref()
            .map(read_assistant_state)
            .unwrap_or_default()
            .poll_count;
        // 会话运行态（last_selected 等）从 conversation.extra.session.state 读；
        // 旧 topic.extra.assistant.last_selected_neuron_id 缺失时回退读取一次（写入走新位置）。
        let session_state = read_session_state(&conversation);
        let last_selected_neuron_id = session_state.last_selected_neuron_id.clone().or_else(|| {
            // 旧数据回退：topic.extra.assistant.last_selected_neuron_id（写入走新位置）。
            topic.as_ref().and_then(|t| {
                t.extra
                    .as_ref()
                    .and_then(|extra| extra.get("assistant"))
                    .and_then(|a| a.get("last_selected_neuron_id"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
        });
        let messages = ModelCallInput::sanitize_tool_pairs(
            &conversation
                .messages
                .iter()
                .filter_map(message_to_model)
                .collect::<Vec<_>>(),
        );
        Ok(AssistantRoundContext {
            session_id: session_id.to_string(),
            topic_id: topic.as_ref().map(|t| t.id.clone()),
            topic_brief: topic.as_ref().map(build_topic_brief),
            trigger,
            user_input: None,
            system_prompt: None,
            selected_neuron: None,
            authorized_tool_ids: Vec::new(),
            messages,
            model_output: None,
            tool_result: None,
            poll_count_for_topic: poll_count,
            last_selected_neuron_id,
            spec_neuron_id: None,
            behavior: None,
        })
    }

    fn topics(&self) -> AppResult<std::sync::MutexGuard<'_, TopicStore>> {
        self.topic_store
            .lock()
            .map_err(|e| AppError::StorageError(format!("TopicStore lock failed: {e}")))
    }

    fn neurons(&self) -> AppResult<std::sync::MutexGuard<'_, NeuronStore>> {
        self.neuron_store
            .lock()
            .map_err(|e| AppError::StorageError(format!("NeuronStore lock failed: {e}")))
    }

    /// 轮询推进计数：poll_count 仍留 topic.extra.assistant（会话运行态已迁至 conversation）。
    fn bump_poll_count(&self, ctx: &AssistantRoundContext) -> AppResult<()> {
        let Some(topic_id) = ctx.topic_id.as_ref() else {
            return Ok(());
        };
        let topic = self
            .topics()?
            .get(topic_id)?
            .ok_or_else(|| AppError::ConversationNotFound(topic_id.clone()))?;
        let mut state = read_assistant_state(&topic);
        state.poll_count = state.poll_count.saturating_add(1);
        write_assistant_state(&self.topic_store, topic_id, state)
    }

    /// 用户干预标记：写会话态（conversation.extra.session.state），topic 不再承载。
    fn mark_user_intervention(&self, ctx: &AssistantRoundContext) -> AppResult<()> {
        let mut state = read_session_state(&self.store.require_conversation(&ctx.session_id)?);
        state.last_intervention_at = Some(now_ms());
        state.intervention_neuron_ids.clear();
        if let Some(neuron) = ctx.selected_neuron.as_ref() {
            state.intervention_neuron_ids.push(neuron.id.clone());
            state.last_selected_neuron_id = Some(neuron.id.clone());
        }
        write_session_state(&self.store, &ctx.session_id, &state)
    }

    fn default_model_or_error(&self) -> AppResult<ChatModelSelection> {
        self.providers
            .default_model_selection()?
            .ok_or(AppError::ModelNotSelected)
    }

    /// 读取 topic 的干预窗口（经会话态）；窗口为空返回空 Vec（由调用方决定跳过或报错）。
    pub fn intervention_window(&self, topic_id: &str) -> AppResult<Vec<String>> {
        let topic = self
            .topics()?
            .get(topic_id)?
            .ok_or_else(|| AppError::ConversationNotFound(topic_id.to_string()))?;
        let Some(session_id) = topic.session_id.clone() else {
            return Ok(Vec::new());
        };
        let conversation = self.store.require_conversation(&session_id)?;
        Ok(read_session_state(&conversation).intervention_neuron_ids)
    }

    /// 对窗口内每个介入神经元应用 delta：节点权重 + 关联边 + lineage 归因 + 变体演进。
    /// 模型打分 hook 与人工评价共用；窗口为空时静默通过。
    pub async fn apply_score_feedback(&self, topic_id: &str, delta: f64) -> AppResult<()> {
        let neuron_ids = self.intervention_window(topic_id)?;
        if neuron_ids.is_empty() {
            return Ok(());
        }
        tracing::info!(
            phase = "apply_score_feedback",
            topic_id,
            neuron_count = neuron_ids.len(),
            delta,
            "applying weight delta"
        );
        for neuron_id in &neuron_ids {
            let _ = self.neuron_manager.adjust_weight(neuron_id, delta)?;
            let connections = self.neurons()?.get_connections(neuron_id)?;
            for edge in connections {
                if edge.target == *neuron_id || edge.source == *neuron_id {
                    let _ =
                        self.neurons()?
                            .adjust_connection_weight(&edge.source, &edge.target, delta);
                }
            }
            // Lineage attribution: the score also flows back to the creator
            // variant that generated this neuron, feeding the self-iteration pool.
            if let Some(parent_id) = self.neurons()?.lineage_parent_id_of(neuron_id)? {
                let _ = self
                    .neuron_manager
                    .accumulate_variant_delta(&parent_id, delta)?;
            }
        }
        // Creator pool self-iteration after a scoring round. Never allowed to
        // break the feedback flow: failures keep the pool unchanged.
        if let Err(error) = self.neuron_manager.maybe_evolve_creator_variants().await {
            tracing::warn!(
                phase = "apply_score_feedback",
                error = %error,
                "maybe_evolve_creator_variants failed; keeping pool unchanged"
            );
        }
        Ok(())
    }

    /// 人工评价入口：按会话解析绑定 topic，校验分数并应用评分 delta。
    /// 与模型打分 hook 共享 `apply_score_feedback`，只是分数来源不同（用户点击 vs 模型 JSON）。
    pub async fn score_feedback(&self, session_id: &str, score: i64) -> AppResult<()> {
        if score == 0 || !(-5..=5).contains(&score) {
            return Err(AppError::InvalidInput(format!(
                "score must be in -5..=5 and non-zero, got {score}"
            )));
        }
        let topic_id = self
            .topics()?
            .find_by_session_id(session_id)?
            .ok_or_else(|| {
                AppError::ConversationNotFound(format!("no topic bound to session {session_id}"))
            })?
            .id;
        let neuron_ids = self.intervention_window(&topic_id)?;
        if neuron_ids.is_empty() {
            return Err(AppError::InvalidInput(
                "no intervention window to score".into(),
            ));
        }
        tracing::info!(
            phase = "manual_score_feedback",
            session_id,
            topic_id = %topic_id,
            score,
            neuron_count = neuron_ids.len(),
            "manual rating applied"
        );
        self.apply_score_feedback(&topic_id, score as f64).await
    }
}

struct AssistantPollHandler {
    tx: UnboundedSender<AssistantStepRequest>,
}

impl PollHandler for AssistantPollHandler {
    fn on_tick(&mut self) {
        tracing::info!(
            phase = "assistant_poll_handler",
            "on_tick fired, sending PollAll"
        );
        let _ = self.tx.send(AssistantStepRequest::PollAll);
    }
}

struct MatchTopicBeforeHook<'a> {
    assistant: &'a AssistantMode,
}

#[async_trait]
impl BeforeHook for MatchTopicBeforeHook<'_> {
    async fn run(&self, ctx: &mut AssistantRoundContext) -> AppResult<()> {
        let model = self.assistant.default_model_or_error()?;
        let unfinished = self.assistant.topics()?.list_unfinished()?;
        tracing::info!(
            phase = "match_topic_hook",
            unfinished = unfinished.len(),
            session_id = %ctx.session_id,
            "calling match-topic model"
        );
        let decision = self
            .assistant
            .call_service
            .call_system_prompt(
                SYSTEM_TYPE_MATCH_TOPIC,
                json!({
                    "user_input": ctx.user_input,
                    "current_session_id": ctx.session_id,
                    "topics": unfinished.iter().map(|t| json!({
                        "id": t.id,
                        "name": t.name,
                        "description": t.description,
                        "status": t.status,
                        "session_id": t.session_id,
                        "progress": t.progress,
                        "scope_in": t.scope_in,
                    })).collect::<Vec<_>>(),
                }),
                &model,
                &ctx.messages,
                true,
            )
            .await?;

        let action = decision
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("create");
        tracing::info!(phase = "match_topic_hook", action, "match decision");
        match action {
            "switch" => {
                let topic_id = decision
                    .get("topic_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AppError::InvalidInput("match topic switch missing topic_id".into())
                    })?;
                let topic = match self.assistant.topics()?.get(topic_id)? {
                    Some(topic) => topic,
                    None => {
                        let created = self
                            .create_bound_topic_from_decision(ctx, &decision, true)
                            .or_else(|error| {
                                tracing::warn!(
                                    phase = "match_topic_hook",
                                    error = %error,
                                    "switch missing and decision lacked scope_in; using emergency scope"
                                );
                                self.create_bound_topic_with_scope(
                                    ctx,
                                    None,
                                    None,
                                    emergency_scope_in(ctx),
                                )
                            })?;
                        tracing::warn!(
                            phase = "match_topic_hook",
                            requested_topic_id = topic_id,
                            created_topic_id = %created.id,
                            "switch target missing; created topic"
                        );
                        ctx.topic_id = Some(created.id);
                        return Ok(());
                    }
                };
                if let Some(bound_session) = topic.session_id.clone() {
                    if bound_session != ctx.session_id {
                        tracing::info!(
                            phase = "match_topic_hook",
                            from_session = %ctx.session_id,
                            to_session = %bound_session,
                            topic_id = %topic.id,
                            "switching session"
                        );
                        ctx.session_id = bound_session;
                        let rebuilt = self
                            .assistant
                            .build_context(&ctx.session_id, ctx.trigger)
                            .await?;
                        ctx.topic_id = rebuilt.topic_id;
                        ctx.messages = rebuilt.messages;
                        ctx.poll_count_for_topic = rebuilt.poll_count_for_topic;
                        ctx.last_selected_neuron_id = rebuilt.last_selected_neuron_id;
                    } else {
                        ctx.topic_id = Some(topic.id);
                    }
                } else {
                    let bound = self
                        .assistant
                        .topics()?
                        .bind_session(&topic.id, &ctx.session_id)?;
                    ctx.topic_id = Some(bound.id);
                }
            }
            _ => {
                if ctx.topic_id.is_none() {
                    let created = self.create_bound_topic_from_decision(ctx, &decision, false)?;
                    tracing::info!(
                        phase = "match_topic_hook",
                        topic_id = %created.id,
                        scope_items = created.scope_in.len(),
                        "created bound topic with scope_in"
                    );
                    ctx.topic_id = Some(created.id);
                }
            }
        }
        Ok(())
    }
}

impl MatchTopicBeforeHook<'_> {
    fn create_bound_topic_from_decision(
        &self,
        ctx: &AssistantRoundContext,
        decision: &serde_json::Value,
        allow_empty_scope_fallback: bool,
    ) -> AppResult<Topic> {
        let fallback_name = default_topic_name(ctx);
        let name = decision
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback_name.as_str())
            .to_string();
        let description = decision
            .get("description")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| ctx.user_input.as_deref().unwrap_or(""))
            .to_string();
        let mut scope_in = parse_scope_in_from_decision(decision)?;
        if scope_in.is_empty() {
            if allow_empty_scope_fallback {
                scope_in = emergency_scope_in(ctx);
            } else {
                return Err(AppError::InvalidInput(
                    "match topic create requires non-empty scope_in with goal and done_contract"
                        .into(),
                ));
            }
        }
        self.create_bound_topic_with_scope(ctx, Some(name), Some(description), scope_in)
    }

    fn create_bound_topic_with_scope(
        &self,
        ctx: &AssistantRoundContext,
        name: Option<String>,
        description: Option<String>,
        scope_in: Vec<ScopeInItem>,
    ) -> AppResult<Topic> {
        let name = name.unwrap_or_else(|| default_topic_name(ctx));
        let description = description.unwrap_or_else(|| ctx.user_input.clone().unwrap_or_default());
        let created = self.assistant.topics()?.create(
            &name,
            &description,
            TopicStatus::Todo,
            scope_in,
            None,
        )?;
        self.assistant
            .topics()?
            .bind_session(&created.id, &ctx.session_id)
    }
}

fn default_topic_name(ctx: &AssistantRoundContext) -> String {
    ctx.user_input
        .as_deref()
        .map(|s| {
            let trimmed = s.trim();
            if trimmed.chars().count() > 40 {
                format!("{}…", trimmed.chars().take(40).collect::<String>())
            } else if trimmed.is_empty() {
                "Assistant Topic".to_string()
            } else {
                trimmed.to_string()
            }
        })
        .unwrap_or_else(|| "Assistant Topic".to_string())
}

fn emergency_scope_in(ctx: &AssistantRoundContext) -> Vec<ScopeInItem> {
    let goal = ctx
        .user_input
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Clarify the topic goal")
        .to_string();
    vec![ScopeInItem {
        id: String::new(),
        goal,
        done_contract: "User confirms the goal and acceptance criteria are clear enough to proceed"
            .into(),
        status: "pending".into(),
    }]
}

fn parse_scope_in_from_decision(decision: &serde_json::Value) -> AppResult<Vec<ScopeInItem>> {
    let Some(value) = decision.get("scope_in") else {
        return Ok(Vec::new());
    };
    let items: Vec<ScopeInItem> = serde_json::from_value(value.clone())
        .map_err(|e| AppError::InvalidInput(format!("match topic invalid scope_in: {e}")))?;
    Ok(items)
}

struct ScoreFeedbackBeforeHook<'a> {
    assistant: &'a AssistantMode,
}

#[async_trait]
impl BeforeHook for ScoreFeedbackBeforeHook<'_> {
    async fn run(&self, ctx: &mut AssistantRoundContext) -> AppResult<()> {
        let Some(topic_id) = ctx.topic_id.clone() else {
            tracing::info!(phase = "score_feedback_hook", "skip: no topic bound yet");
            return Ok(());
        };
        let topic = match self.assistant.topics()?.get(&topic_id)? {
            Some(topic) => topic,
            None => {
                tracing::info!(phase = "score_feedback_hook", topic_id = %topic_id, "skip: topic missing");
                return Ok(());
            }
        };
        // 干预窗口自会话态读取（旧 topic.extra.assistant 数据已迁移）。
        let Some(session_id) = topic.session_id.clone() else {
            tracing::info!(phase = "score_feedback_hook", topic_id = %topic_id, "skip: topic not bound to session");
            return Ok(());
        };
        let conversation = match self.assistant.store.require_conversation(&session_id) {
            Ok(conversation) => conversation,
            Err(_) => {
                tracing::info!(phase = "score_feedback_hook", topic_id = %topic_id, "skip: conversation missing");
                return Ok(());
            }
        };
        let state = read_session_state(&conversation);
        if state.last_intervention_at.is_none() || state.intervention_neuron_ids.is_empty() {
            tracing::info!(
                phase = "score_feedback_hook",
                topic_id = %topic_id,
                "skip: no prior intervention window"
            );
            return Ok(());
        }
        tracing::info!(
            phase = "score_feedback_hook",
            topic_id = %topic_id,
            neuron_count = state.intervention_neuron_ids.len(),
            "scoring intervention window"
        );
        let model = self.assistant.default_model_or_error()?;
        let decision = match self
            .assistant
            .call_service
            .call_system_prompt(
                SYSTEM_TYPE_SCORE_FEEDBACK,
                json!({
                    "user_input": ctx.user_input,
                    "topic_id": topic_id,
                    "neuron_ids": state.intervention_neuron_ids,
                }),
                &model,
                &ctx.messages,
                true,
            )
            .await
        {
            Ok(decision) => decision,
            Err(e) => {
                // 程序强制规范兜底：模型未返回合法 JSON 时，跳过打分，
                // 不让评分副作用阻断主对话（见 assistant.score_feedback.md）。
                tracing::warn!(
                    phase = "score_feedback_hook",
                    topic_id = %topic_id,
                    error = %e,
                    "json parse failed; skip scoring instead of failing the round"
                );
                return Ok(());
            }
        };
        let score = decision
            .get("score")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| AppError::InvalidInput("score feedback missing score".into()))?;
        if score == 0 || !(-5..=5).contains(&score) {
            return Err(AppError::InvalidInput(format!(
                "score must be in -5..=5 and non-zero, got {score}"
            )));
        }
        tracing::info!(
            phase = "score_feedback_hook",
            score,
            "applying weight delta"
        );
        // 与人工评价共用同一评分逻辑：节点权重 + 关联边 + lineage 归因 + 变体演进。
        self.assistant
            .apply_score_feedback(&topic_id, score as f64)
            .await
    }
}

struct CompleteScopeAfterHook<'a> {
    assistant: &'a AssistantMode,
}

#[async_trait]
impl AfterHook for CompleteScopeAfterHook<'_> {
    async fn run(&self, ctx: &mut AssistantRoundContext) -> AppResult<()> {
        let Some(topic_id) = ctx.topic_id.clone() else {
            tracing::info!(phase = "complete_scope_hook", "skip: no topic");
            return Ok(());
        };
        let topic = match self.assistant.topics()?.get(&topic_id)? {
            Some(topic) => topic,
            None => {
                tracing::info!(phase = "complete_scope_hook", topic_id = %topic_id, "skip: topic missing");
                return Ok(());
            }
        };
        if topic.scope_in.is_empty() {
            tracing::info!(
                phase = "complete_scope_hook",
                topic_id = %topic_id,
                "skip: empty scope_in"
            );
            return Ok(());
        }
        tracing::info!(
            phase = "complete_scope_hook",
            topic_id = %topic_id,
            scope_items = topic.scope_in.len(),
            "calling complete-scope model"
        );
        let model = self.assistant.default_model_or_error()?;
        let decision = self
            .assistant
            .call_service
            .call_system_prompt(
                SYSTEM_TYPE_COMPLETE_SCOPE,
                json!({
                    "topic_id": topic_id,
                    "scope_in": topic.scope_in,
                    "model_output": ctx.model_output,
                    "tool_result": ctx.tool_result,
                    "user_input": ctx.user_input,
                }),
                &model,
                &ctx.messages,
                true,
            )
            .await?;
        let ids = decision
            .get("completed_item_ids")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        tracing::info!(
            phase = "complete_scope_hook",
            completed = ids.len(),
            "completing scope items"
        );
        for id in ids {
            let Some(item_id) = id.as_str() else {
                continue;
            };
            let _ = self
                .assistant
                .topics()?
                .complete_scope_item(&topic_id, item_id);
        }
        Ok(())
    }
}

/// topic 侧残留的助手状态：仅轮询计数（会话运行态已迁至 conversation.extra.session.state）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AssistantTopicState {
    #[serde(default)]
    poll_count: u64,
}

fn read_assistant_state(topic: &Topic) -> AssistantTopicState {
    topic
        .extra
        .as_ref()
        .and_then(|extra| extra.get("assistant"))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

fn write_assistant_state(
    topic_store: &Arc<Mutex<TopicStore>>,
    topic_id: &str,
    state: AssistantTopicState,
) -> AppResult<()> {
    let store = topic_store
        .lock()
        .map_err(|e| AppError::StorageError(format!("TopicStore lock failed: {e}")))?;
    let topic = store
        .get(topic_id)?
        .ok_or_else(|| AppError::ConversationNotFound(topic_id.to_string()))?;
    let mut extra = topic.extra.unwrap_or_else(|| json!({}));
    if !extra.is_object() {
        extra = json!({});
    }
    extra
        .as_object_mut()
        .unwrap()
        .insert("assistant".into(), serde_json::to_value(state).unwrap());
    store.update(
        topic_id,
        TopicUpdate {
            extra: Some(Some(extra)),
            ..Default::default()
        },
    )?;
    Ok(())
}

pub fn extract_json_object(text: &str) -> AppResult<serde_json::Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if value.is_object() {
            return Ok(value);
        }
    }
    let start = trimmed
        .find('{')
        .ok_or_else(|| AppError::InvalidInput("LLM response missing JSON object".into()))?;
    let end = trimmed
        .rfind('}')
        .ok_or_else(|| AppError::InvalidInput("LLM response missing JSON object end".into()))?;
    if end < start {
        return Err(AppError::InvalidInput(
            "LLM response has invalid JSON object bounds".into(),
        ));
    }
    serde_json::from_str(&trimmed[start..=end])
        .map_err(|e| AppError::InvalidInput(format!("Failed to parse LLM JSON: {e}")))
}

/// 为轮询 / 手动推进回合构建课题简报：目标、进度与 scope_in 待办清单（含验收标准）。
fn build_topic_brief(topic: &Topic) -> String {
    let mut out = String::from("【课题简报】\n");
    out.push_str(&format!("课题：{}\n", topic.name.trim()));
    if !topic.description.trim().is_empty() {
        out.push_str(&format!("目标：{}\n", topic.description.trim()));
    }
    out.push_str(&format!("进度：{}%\n", topic.progress));
    out.push_str("待办清单：\n");
    if topic.scope_in.is_empty() {
        out.push_str("- （无待办项）\n");
    } else {
        for item in &topic.scope_in {
            let mark = if item.status == "completed" {
                "[x]"
            } else {
                "[ ]"
            };
            out.push_str(&format!(
                "- {mark} {}\n    验收：{}\n",
                item.goal.trim(),
                item.done_contract.trim()
            ));
        }
    }
    out.push_str(
        "本轮任务：基于上述课题，选择一件尚未完成的事项推进；必要时调用可用工具执行，并在回复中说明本轮进展。若所有事项均已完成，输出完成总结。",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::call_service::filter_authorized_tool_ids;
    use crate::core::tool_registry::ToolRegistry;

    #[test]
    fn filter_drops_unknown_tool_ids() {
        let registry = ToolRegistry::new();
        let filtered = filter_authorized_tool_ids(
            &registry,
            &["echo".into(), "missing_tool".into(), "calculate".into()],
        );
        assert!(filtered.is_empty());
    }

    #[test]
    fn extract_json_from_fenced_text() {
        let value = extract_json_object("here\n```json\n{\"score\": 2}\n```").unwrap();
        assert_eq!(value["score"], 2);
    }
}

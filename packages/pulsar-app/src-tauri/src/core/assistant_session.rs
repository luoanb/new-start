//! Assistant 业务接入（独立文件，业务逻辑不进入 Gateway 正文）。
//!
//! `AssistantSession` 提供 `converse` / `step` / `step_poller` 入口与轮询调度壳；
//! `AssistantHooks` 实现 [`RoundHooks`]（注入到 `ConversationRunner` 的单轮生命周期），
//! 承载课题副作用：干预打分（score_feedback）、课题匹配/创建/切换（match_topic）、
//! 进度验收（complete_scope）、用户干预标记与轮询计数。

use std::fmt;
use std::sync::{
    atomic::Ordering,
    Arc, Mutex, MutexGuard,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;

use super::{
    call_service::{
        read_session_state, write_session_state, NeuronCallService, RoundInput, SessionSeed,
        SessionState,
    },
    conversation_runner::{
        ConversationRunner, InputRecord, RoundContext, RoundHooks, RoundTriggerKind,
    },
    conversation_store::{now_ms, ConversationStore},
    error::{AppError, AppResult},
    models::{
        ChatModelSelection, ChatResponse, EnsureSystemOpts, ModelMessage, ScopeInItem, Topic,
        TopicStatus, TopicUpdate,
    },
    neuron::model::extract_json_object,
    neuron_manager::NeuronManager,
    neuron_store::NeuronStore,
    poller::{Poller, SharedPollParallelism},
    poller_step::{AssistantPollHandler, AssistantStepRequest, ASSISTANT_POLL_TASK},
    providers::ProviderRegistry,
    session_tracker::SessionTracker,
    topic_store::TopicStore,
};

pub const SYSTEM_TYPE_SELECT_NEURON: &str = "assistant_select_neuron";
pub const SYSTEM_TYPE_MATCH_TOPIC: &str = "assistant_match_topic";
pub const SYSTEM_TYPE_COMPLETE_SCOPE: &str = "assistant_complete_scope";
pub const SYSTEM_TYPE_SCORE_FEEDBACK: &str = "assistant_score_feedback";

/// Re-export default interval ticks (overridable via `config.json` → `poller`).
pub use super::poller::DEFAULT_ASSISTANT_POLL_TICKS;

/// Assistant 业务门面：对话 / 手动推进 / 轮询推进 + 轮询调度壳。
pub struct AssistantSession {
    store: ConversationStore,
    providers: ProviderRegistry,
    neuron_manager: Arc<NeuronManager>,
    topic_store: Arc<Mutex<TopicStore>>,
    neuron_store: Arc<Mutex<NeuronStore>>,
    /// 单轮编排：读会话 → before hooks → converse → after hooks → 落库。
    runner: ConversationRunner,
    /// 无状态单轮对话引擎：裁决调用（call_judgement）与主对话共用同一执行入口。
    call_service: Arc<NeuronCallService>,
    step_tx: UnboundedSender<AssistantStepRequest>,
    session_tracker: SessionTracker,
    /// 与 Poller 共享的轮询并发推进数量（运行时可变，前端可调）。
    poll_parallelism: SharedPollParallelism,
}

impl fmt::Debug for AssistantSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssistantSession").finish_non_exhaustive()
    }
}

impl AssistantSession {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: ConversationStore,
        providers: ProviderRegistry,
        neuron_manager: Arc<NeuronManager>,
        topic_store: Arc<Mutex<TopicStore>>,
        neuron_store: Arc<Mutex<NeuronStore>>,
        runner: ConversationRunner,
        call_service: Arc<NeuronCallService>,
        step_tx: UnboundedSender<AssistantStepRequest>,
        session_tracker: SessionTracker,
        poll_parallelism: SharedPollParallelism,
    ) -> Self {
        Self {
            store,
            providers,
            neuron_manager,
            topic_store,
            neuron_store,
            runner,
            call_service,
            step_tx,
            session_tracker,
            poll_parallelism,
        }
    }

    /// 裁决类系统提示词调用：懒创建系统神经元 → 用 [`NeuronCallService::converse`] 跑一轮
    /// （系统类型 seed + 禁工具 + 无会话态）→ 解析 JSON 决策。
    ///
    /// 取代旧 `NeuronManager::call_system_prompt`：裁决语义即 converse 的一种调用形态，
    /// 模型调用统一收敛到 `converse` 唯一公共入口，NeuronManager 回归纯管理面。
    async fn call_judgement(
        &self,
        system_type: &str,
        user_payload: serde_json::Value,
        model: &ChatModelSelection,
        history: &[ModelMessage],
    ) -> AppResult<serde_json::Value> {
        let spec = self
            .neuron_manager
            .ensure_system_neuron(system_type, EnsureSystemOpts { reset: false })
            .await?;
        let outcome = self
            .call_service
            .converse(
                RoundInput {
                    seed: Some(SessionSeed::Neuron(spec.id)),
                    state: SessionState::default(),
                    messages: history.to_vec(),
                    tool_override: Some(Vec::new()),
                },
                &user_payload.to_string(),
                model,
            )
            .await?;
        extract_json_object(&outcome.response)
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

    /// 用户主对话：User 触发一轮（score_feedback + match_topic + complete_scope + 干预标记）。
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
        let hooks = AssistantHooks { assistant: self };
        let response = self
            .runner
            .run_round(
                session_id,
                InputRecord::User(user_input.to_string()),
                None,
                model,
                Some(&hooks),
            )
            .await?;
        tracing::info!(
            phase = "assistant_converse",
            session_id = %response.conversation_id,
            response_len = response.response.len(),
            "converse ok"
        );
        Ok(response)
    }

    /// 手动推进一轮（ManualStep）：需课题已绑定，简报作为本轮指令。
    pub async fn step(
        &self,
        session_id: &str,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        tracing::info!(phase = "assistant_step", session_id, "step start");
        let hooks = AssistantHooks { assistant: self };
        let response = self
            .runner
            .run_round(session_id, InputRecord::None, None, model, Some(&hooks))
            .await?;
        tracing::info!(phase = "assistant_step", session_id, "step ok");
        Ok(response)
    }

    /// 轮询推进一轮（Poller）：落 nudge 消息，简报作为本轮指令。
    pub async fn step_poller(
        &self,
        session_id: &str,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        tracing::info!(phase = "assistant_poller", session_id, "poller step start");
        let hooks = AssistantHooks { assistant: self };
        self.runner
            .run_round(session_id, InputRecord::Nudge, None, model, Some(&hooks))
            .await
    }

    pub fn register_polling(&self, poller: &mut Poller, interval_ticks: u64) -> AppResult<()> {
        let tx = self.step_tx.clone();
        poller.register(
            ASSISTANT_POLL_TASK,
            interval_ticks.max(1),
            Box::new(AssistantPollHandler { tx }),
        )
    }

    /// 轮询调度壳：PollAll → 跨课题受限并发推进（互不干扰、信号量限流）。
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

    fn topics(&self) -> AppResult<MutexGuard<'_, TopicStore>> {
        self.topic_store
            .lock()
            .map_err(|e| AppError::StorageError(format!("TopicStore lock failed: {e}")))
    }

    fn neurons(&self) -> AppResult<MutexGuard<'_, NeuronStore>> {
        self.neuron_store
            .lock()
            .map_err(|e| AppError::StorageError(format!("NeuronStore lock failed: {e}")))
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

/// 注入 `ConversationRunner` 的单轮钩子：承载 Assistant 全部课题副作用。
struct AssistantHooks<'a> {
    assistant: &'a AssistantSession,
}

#[async_trait]
impl RoundHooks for AssistantHooks<'_> {
    async fn before_round(&self, ctx: &mut RoundContext) -> AppResult<()> {
        // 会话可能已绑定课题（第二轮起的 User 输入 / 手动 / 轮询推进）。
        self.resolve_bound_topic(ctx)?;
        match ctx.trigger {
            RoundTriggerKind::User => {
                self.score_feedback(ctx).await?;
                self.match_topic(ctx).await?;
            }
            RoundTriggerKind::ManualStep | RoundTriggerKind::Poller => {
                // 推进型触发需课题已绑定，简报作为本轮指令。
                let topic_id = ctx.topic_id.as_ref().ok_or_else(|| {
                    AppError::InvalidInput(
                        "Assistant step requires a topic bound to the session".into(),
                    )
                })?;
                let topic = self.assistant.topics()?.get(topic_id)?.ok_or_else(|| {
                    AppError::ConversationNotFound(topic_id.clone())
                })?;
                ctx.model_input = build_topic_brief(&topic);
            }
            RoundTriggerKind::AgentLoop => {
                unreachable!("assistant hooks never run agent-loop rounds")
            }
        }
        Ok(())
    }

    async fn after_round(&self, ctx: &mut RoundContext) -> AppResult<()> {
        let completed = self.complete_scope(ctx).await;
        match ctx.trigger {
            RoundTriggerKind::User => {
                completed?;
                self.mark_user_intervention(ctx)?;
            }
            RoundTriggerKind::ManualStep => {
                completed?;
                self.bump_poll_count(ctx)?;
            }
            RoundTriggerKind::Poller => {
                // 轮询推进不得被课题副作用打断（失败仅记录）。
                if let Err(error) = completed {
                    tracing::error!(
                        phase = "assistant_poller",
                        error = %error,
                        "complete_scope afterhook failed; ignored"
                    );
                }
                let _ = self.bump_poll_count(ctx);
            }
            RoundTriggerKind::AgentLoop => {
                unreachable!("assistant hooks never run agent-loop rounds")
            }
        }
        Ok(())
    }
}

impl AssistantHooks<'_> {
    /// 若当前未指定课题，则按会话解析已绑定课题（不存在保持 None）。
    fn resolve_bound_topic(&self, ctx: &mut RoundContext) -> AppResult<()> {
        if ctx.topic_id.is_some() {
            return Ok(());
        }
        ctx.topic_id = self
            .assistant
            .topics()?
            .find_by_session_id(&ctx.session_id)?
            .map(|topic| topic.id);
        Ok(())
    }

    /// 干预打分：会话态存在干预窗口时调用模型打分（解析失败仅 warn + skip，不阻断主对话）。
    async fn score_feedback(&self, ctx: &mut RoundContext) -> AppResult<()> {
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
            .call_judgement(
                SYSTEM_TYPE_SCORE_FEEDBACK,
                json!({
                    "user_input": ctx.model_input,
                    "topic_id": topic_id,
                    "neuron_ids": state.intervention_neuron_ids,
                }),
                &model,
                &ctx.messages,
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
        tracing::info!(phase = "score_feedback_hook", score, "applying weight delta");
        self.assistant
            .apply_score_feedback(&topic_id, score as f64)
            .await
    }

    /// 课题匹配/创建/切换：模型裁决 action（switch → 已有课题；create → 新建课题）。
    async fn match_topic(&self, ctx: &mut RoundContext) -> AppResult<()> {
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
            .call_judgement(
                SYSTEM_TYPE_MATCH_TOPIC,
                json!({
                    "user_input": ctx.model_input,
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
                        // 切换到目标课题绑定的会话：runner 检测到 session_id 变化后自动 reload。
                        tracing::info!(
                            phase = "match_topic_hook",
                            from_session = %ctx.session_id,
                            to_session = %bound_session,
                            topic_id = %topic.id,
                            "switching session"
                        );
                        ctx.session_id = bound_session;
                        ctx.topic_id = Some(topic.id);
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

    /// 进度验收：scope_in 非空时调用模型裁决已完成项并落库（失败仅记录，不阻断）。
    async fn complete_scope(&self, ctx: &mut RoundContext) -> AppResult<()> {
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
        let outcome = ctx
            .outcome
            .as_ref()
            .ok_or_else(|| AppError::InvalidInput("complete_scope requires a finished round".into()))?;
        let model_output = outcome.model_output.clone();
        let tool_result = outcome.tool_result.clone();
        tracing::info!(
            phase = "complete_scope_hook",
            topic_id = %topic_id,
            scope_items = topic.scope_in.len(),
            "calling complete-scope model"
        );
        let model = self.assistant.default_model_or_error()?;
        let decision = self
            .assistant
            .call_judgement(
                SYSTEM_TYPE_COMPLETE_SCOPE,
                json!({
                    "topic_id": topic_id,
                    "scope_in": topic.scope_in,
                    "model_output": model_output,
                    "tool_result": tool_result,
                    "user_input": ctx.model_input,
                }),
                &model,
                &ctx.messages,
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

    /// 用户干预标记：写会话态（conversation.extra.session.state），topic 不再承载。
    fn mark_user_intervention(&self, ctx: &RoundContext) -> AppResult<()> {
        let mut state = read_session_state(&self.assistant.store.require_conversation(&ctx.session_id)?);
        state.last_intervention_at = Some(now_ms());
        state.intervention_neuron_ids.clear();
        if let Some(neuron_id) = ctx
            .outcome
            .as_ref()
            .and_then(|outcome| outcome.selected_neuron_id.clone())
        {
            state.intervention_neuron_ids.push(neuron_id.clone());
            state.last_selected_neuron_id = Some(neuron_id);
        }
        write_session_state(&self.assistant.store, &ctx.session_id, &state)
    }

    /// 轮询推进计数：poll_count 仍留 topic.extra.assistant（会话运行态已迁至 conversation）。
    fn bump_poll_count(&self, ctx: &RoundContext) -> AppResult<()> {
        let Some(topic_id) = ctx.topic_id.as_ref() else {
            return Ok(());
        };
        let topic = self
            .assistant
            .topics()?
            .get(topic_id)?
            .ok_or_else(|| AppError::ConversationNotFound(topic_id.clone()))?;
        let mut state = read_assistant_state(&topic);
        state.poll_count = state.poll_count.saturating_add(1);
        write_assistant_state(&self.assistant.topic_store, topic_id, state)
    }

    fn create_bound_topic_from_decision(
        &self,
        ctx: &RoundContext,
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
            .unwrap_or_else(|| ctx.model_input.as_str())
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
        ctx: &RoundContext,
        name: Option<String>,
        description: Option<String>,
        scope_in: Vec<ScopeInItem>,
    ) -> AppResult<Topic> {
        let name = name.unwrap_or_else(|| default_topic_name(ctx));
        let description = description.unwrap_or_else(|| ctx.model_input.clone());
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

fn default_topic_name(ctx: &RoundContext) -> String {
    let input = ctx.model_input.trim();
    if input.is_empty() {
        return "Assistant Topic".to_string();
    }
    if input.chars().count() > 40 {
        format!("{}…", input.chars().take(40).collect::<String>())
    } else {
        input.to_string()
    }
}

fn emergency_scope_in(ctx: &RoundContext) -> Vec<ScopeInItem> {
    let goal = ctx
        .model_input
        .trim()
        .to_string();
    vec![ScopeInItem {
        id: String::new(),
        goal: if goal.is_empty() {
            "Clarify the topic goal".into()
        } else {
            goal
        },
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

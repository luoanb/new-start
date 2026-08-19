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
    conversation_runner::{
        ConversationRunner, InputRecord, RoundContext, RoundHooks, RoundTriggerKind, StreamDelta,
    },
    conversation_store::ConversationStore,
    error::{AppError, AppResult},
    models::{
        ChatModelSelection, ChatResponse, EnsureSystemOpts, Message, MessageBody, MessageRole,
        ScopeInItem, ThinkingConfig, Topic, TopicStatus, TopicUpdate,
    },
    neuron::model::extract_json_object,
    neuron_manager::NeuronManager,
    neuron_store::NeuronStore,
    poller::{Poller, SharedPollParallelism},
    poller_step::{AssistantPollHandler, AssistantStepRequest, ASSISTANT_POLL_TASK},
    round_types::SessionSeed,
    session_tracker::SessionTracker,
    topic_store::TopicStore,
};

pub const SYSTEM_TYPE_SELECT_NEURON: &str = "assistant_select_neuron";
pub const SYSTEM_TYPE_MATCH_TOPIC: &str = "assistant_match_topic";
pub const SYSTEM_TYPE_COMPLETE_SCOPE: &str = "assistant_complete_scope";
pub const SYSTEM_TYPE_SCORE_FEEDBACK: &str = "assistant_score_feedback";
pub const SYSTEM_TYPE_REVISE_TOPIC: &str = "assistant_revise_topic";

/// Re-export default interval ticks (overridable via `config.json` → `poller`).
pub use super::poller::DEFAULT_ASSISTANT_POLL_TICKS;

/// Assistant 业务门面：对话 / 手动推进 / 轮询推进 + 轮询调度壳。
pub struct AssistantSession {
    store: ConversationStore,
    neuron_manager: Arc<NeuronManager>,
    topic_store: Arc<Mutex<TopicStore>>,
    neuron_store: Arc<Mutex<NeuronStore>>,
    /// 单轮编排：读会话 → before hooks → 三段管道 → after hooks → 落库。
    /// 裁决调用（call_judgement）经 `run_raw_round` 与主对话共用同一三段管道。
    runner: ConversationRunner,
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
        neuron_manager: Arc<NeuronManager>,
        topic_store: Arc<Mutex<TopicStore>>,
        neuron_store: Arc<Mutex<NeuronStore>>,
        runner: ConversationRunner,
        step_tx: UnboundedSender<AssistantStepRequest>,
        session_tracker: SessionTracker,
        poll_parallelism: SharedPollParallelism,
    ) -> Self {
        Self {
            store,
            neuron_manager,
            topic_store,
            neuron_store,
            runner,
            step_tx,
            session_tracker,
            poll_parallelism,
        }
    }

    /// 裁决类系统提示词调用：懒创建系统神经元 → 用 [`ConversationRunner::run_raw_round`]
    /// 跑一轮原始管道（系统类型 seed + 禁工具 + 无会话态）→ 解析 JSON 决策。
    ///
    /// 取代旧 `NeuronManager::call_system_prompt`：裁决语义即单轮管道的一种调用形态，
    /// 模型调用统一收敛到 `run_raw_round` 唯一公共入口，NeuronManager 回归纯管理面。
    async fn call_judgement(
        &self,
        system_type: &str,
        user_payload: serde_json::Value,
        model: &ChatModelSelection,
        history: &[Message],
    ) -> AppResult<serde_json::Value> {
        tracing::info!(
            phase = "call_judgement",
            system_type,
            provider = %model.provider_id,
            model = %model.model_id,
            history = history.len(),
            payload_len = user_payload.to_string().len(),
            "judgement call start"
        );
        let spec = self
            .neuron_manager
            .ensure_system_neuron(system_type, EnsureSystemOpts { reset: false })
            .await?;
        let outcome = self
            .runner
            .run_raw_round(
                Some(SessionSeed::Neuron(spec.id)),
                None,
                history.to_vec(),
                &user_payload.to_string(),
                Some(Vec::new()),
                // 裁决调用非对话：不注入任何标签工具（禁工具语义保持不变）。
                Vec::new(),
                true,
                model,
                // 裁决 hook：非对话调用，直接显式关闭深度思考。
                Some(ThinkingConfig {
                    enabled: Some(false),
                    effort: None,
                }),
            )
            .await?;
        let decision = extract_json_object(&outcome.response).map_err(|error| {
            // 裁决调用要求模型只输出 JSON；解析失败时留痕原始输出（截断），便于定位
            // 「LLM response missing JSON object」类问题（模型偶发输出散文而非 JSON）。
            tracing::warn!(
                phase = "call_judgement",
                system_type,
                response_len = outcome.response.len(),
                response_preview = outcome.response.chars().take(500).collect::<String>(),
                error = %error,
                "judgement JSON parse failed; dumping raw model output"
            );
            error
        })?;
        tracing::info!(
            phase = "call_judgement",
            system_type,
            decision = %decision,
            "judgement call done"
        );
        Ok(decision)
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
                None, // 用户聊天窗口发起：保留思考配置（跟随前端勾选）
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

    /// 流式版 `converse`：逐块回调 `on_delta`（Gateway 转发为 `MessageDelta`），
    /// 课题 hooks（score/match/complete）与阻塞版完全一致。
    pub async fn converse_stream(
        &self,
        session_id: &str,
        user_input: &str,
        model: &ChatModelSelection,
        on_delta: Option<Box<dyn FnMut(StreamDelta) + Send>>,
    ) -> AppResult<ChatResponse> {
        tracing::info!(
            phase = "assistant_converse_stream",
            session_id,
            provider = %model.provider_id,
            model = %model.model_id,
            input_len = user_input.len(),
            "converse stream start"
        );
        let hooks = AssistantHooks { assistant: self };
        let response = self
            .runner
            .run_round_stream(
                session_id,
                InputRecord::User(user_input.to_string()),
                None,
                model,
                Some(&hooks),
                None, // 用户聊天窗口发起：保留思考配置（跟随前端勾选）
                on_delta,
            )
            .await?;
        tracing::info!(
            phase = "assistant_converse_stream",
            session_id = %response.conversation_id,
            response_len = response.response.len(),
            "converse stream ok"
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
            .run_round(
                session_id,
                InputRecord::None,
                None,
                model,
                Some(&hooks),
                // 手动推进：非对话调用，直接显式关闭深度思考。
                Some(ThinkingConfig {
                    enabled: Some(false),
                    effort: None,
                }),
            )
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
            .run_round(
                session_id,
                InputRecord::Nudge,
                None,
                model,
                Some(&hooks),
                // 轮询推进：非对话调用，直接显式关闭深度思考。
                Some(ThinkingConfig {
                    enabled: Some(false),
                    effort: None,
                }),
            )
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
    /// 返回实际推进（register 成功）的会话 id 列表；空转（无未完成课题 / 全部
    /// 被跳过）返回空 Vec，调用方据此决定是否广播刷新事件，避免无效通知。
    pub async fn process_step_request(
        self: Arc<Self>,
        request: AssistantStepRequest,
        model: &ChatModelSelection,
    ) -> Vec<String> {
        match request {
            AssistantStepRequest::PollAll => {
                tracing::info!(
                    phase = "assistant_poll_handler",
                    "PollAll received in process_step_request"
                );
                let topics = match self.topics().and_then(|store| store.list_unfinished()) {
                    Ok(topics) => topics,
                    Err(error) => {
                        tracing::error!(
                            phase = "assistant_poll_handler",
                            error = %error,
                            "PollAll topic list failed"
                        );
                        return Vec::new();
                    }
                };
                tracing::info!(
                    phase = "assistant_poll_handler",
                    topic_count = topics.len(),
                    "PollAll topic list resolved"
                );

                let parallelism = self.poll_parallelism.load(Ordering::Relaxed).max(1);
                let semaphore = Arc::new(tokio::sync::Semaphore::new(parallelism));
                let touched = Arc::new(Mutex::new(Vec::<String>::new()));
                let mut tasks = tokio::task::JoinSet::new();
                let mut skipped = 0usize;
                for topic in topics {
                    let Some(session_id) = topic.session_id else {
                        skipped += 1;
                        continue;
                    };
                    if skip_polling(&topic.status) {
                        skipped += 1;
                        continue;
                    }
                    let topic_id = topic.id;
                    // 跳过已在运行的会话（用户手动 converse 推进中 / 上一批尚未收尾），
                    // 避免对同一会话重复发起推进。
                    if let Ok(Some(_)) = self.session_tracker.get(&session_id) {
                        tracing::info!(
                            phase = "assistant_poll_handler",
                            topic_id,
                            session_id,
                            "skip topic: session already running"
                        );
                        skipped += 1;
                        continue;
                    }
                    let model = model.clone();
                    let assistant = Arc::clone(&self);
                    let semaphore = Arc::clone(&semaphore);
                    let touched = Arc::clone(&touched);
                    tasks.spawn(async move {
                        let _permit = semaphore.acquire().await.expect("semaphore not closed");
                        if let Err(error) = assistant.session_tracker.register(&session_id, None) {
                            tracing::error!(
                                phase = "assistant_poll_handler",
                                topic_id,
                                session_id,
                                error = %error,
                                "poll register failed"
                            );
                            return;
                        }
                        let _ = assistant
                            .session_tracker
                            .update_step(&session_id, "polling");
                        match assistant.step_poller(&session_id, &model).await {
                            Ok(response) => tracing::info!(
                                phase = "assistant_poll_handler",
                                topic_id,
                                session_id,
                                response_len = response.response.len(),
                                "poll step ok"
                            ),
                            Err(error) => tracing::error!(
                                phase = "assistant_poll_handler",
                                topic_id,
                                session_id,
                                error = %error,
                                "poll step failed"
                            ),
                        }
                        assistant.session_tracker.unregister(&session_id);
                        if let Ok(mut list) = touched.lock() {
                            list.push(session_id);
                        }
                    });
                }
                tracing::info!(
                    phase = "assistant_poll_handler",
                    spawned = tasks.len(),
                    skipped,
                    parallelism,
                    "PollAll tasks spawned"
                );
                while tasks.join_next().await.is_some() {}
                let touched = touched.lock().map(|list| list.clone()).unwrap_or_default();
                tracing::info!(
                    phase = "assistant_poll_handler",
                    touched_sessions = touched.len(),
                    "PollAll finished"
                );
                touched
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

    /// 对显式神经元集合应用 delta：节点权重 + 关联边 + lineage 归因 + 变体演进。
    /// 模型打分 hook 与人工评价共用；集合为空时静默通过。
    pub async fn apply_score_feedback(
        &self,
        topic_id: &str,
        neuron_ids: Vec<String>,
        delta: f64,
    ) -> AppResult<()> {
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
            // 注意：先解绑再 if let——if let scrutinee 里的临时 MutexGuard 会
            // 因 temporary lifetime extension 存活到块结束，块内再锁同一 store 会重入死锁。
            let parent_id = self.neurons()?.lineage_parent_id_of(neuron_id)?;
            if let Some(parent_id) = parent_id {
                let _ = self
                    .neuron_manager
                    .accumulate_variant_delta(&parent_id, delta)?;
            }
        }
        // Creator pool self-iteration after a scoring round. Never allowed to
        // break the feedback flow: failures keep the pool unchanged.
        tracing::info!(phase = "apply_score_feedback", "calling maybe_evolve_creator_variants");
        if let Err(error) = self.neuron_manager.maybe_evolve_creator_variants().await {
            tracing::warn!(
                phase = "apply_score_feedback",
                error = %error,
                "maybe_evolve_creator_variants failed; keeping pool unchanged"
            );
        }
        tracing::info!(phase = "apply_score_feedback", "apply score feedback done");
        Ok(())
    }

    /// 人工评价入口：按会话解析绑定 topic，定位被评消息所在介入区间并应用评分 delta。
    /// 区间 = 上次用户介入（不含）之后、下次介入（不含）之前的所有盖章神经元（去重），
    /// 与模型打分共用 `apply_score_feedback`，仅分数来源不同（用户点击 vs 模型 JSON）。
    pub async fn score_feedback(
        &self,
        session_id: &str,
        message_index: usize,
        score: i64,
    ) -> AppResult<()> {
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
        let conversation = self.store.require_conversation(session_id)?;
        if message_index >= conversation.messages.len() {
            return Err(AppError::InvalidInput(format!(
                "message_index {message_index} out of range (len {})",
                conversation.messages.len()
            )));
        }
        let neuron_ids = interval_neuron_ids(&conversation.messages, message_index);
        if neuron_ids.is_empty() {
            return Err(AppError::InvalidInput(
                "当前消息所在介入区间内没有可评分的神经元（该区间未选中任何神经元）".into(),
            ));
        }
        tracing::info!(
            phase = "manual_score_feedback",
            session_id,
            topic_id = %topic_id,
            message_index,
            score,
            neuron_count = neuron_ids.len(),
            "manual rating applied"
        );
        self.apply_score_feedback(&topic_id, neuron_ids, score as f64).await
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
                // 用户接入即解除等待用户状态（blocked 项 → pending，恢复课题轮询）。
                self.release_waiting_user(ctx)?;
                self.score_feedback(ctx).await?;
                self.match_topic(ctx).await?;
            }
            RoundTriggerKind::ManualStep | RoundTriggerKind::Poller => {
                self.advance_brief(ctx)?;
            }
            RoundTriggerKind::AgentLoop => {
                unreachable!("assistant hooks never run agent-loop rounds")
            }
        }
        Ok(())
    }

    async fn after_round(&self, ctx: &mut RoundContext) -> AppResult<()> {
        // 先改内容再验收：revise_topic（范围修订）先于 complete_scope（进度验收）执行，
        // 新加/修订项本轮即可参与验收勾选。
        let revised = self.revise_topic(ctx).await;
        let completed = self.complete_scope(ctx).await;
        match ctx.trigger {
            RoundTriggerKind::User => {
                revised?;
                completed?;
                self.tick_round_counters(ctx, true)?;
            }
            RoundTriggerKind::ManualStep => {
                revised?;
                completed?;
                self.tick_round_counters(ctx, false)?;
            }
            RoundTriggerKind::Poller => {
                // 轮询推进不得被课题副作用打断（失败仅记录）。
                if let Err(error) = revised {
                    tracing::error!(
                        phase = "assistant_poller",
                        error = %error,
                        "revise_topic afterhook failed; ignored"
                    );
                }
                if let Err(error) = completed {
                    tracing::error!(
                        phase = "assistant_poller",
                        error = %error,
                        "complete_scope afterhook failed; ignored"
                    );
                }
                let _ = self.tick_round_counters(ctx, false);
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

    /// 用户接入：解除课题内全部 blocked 项（等待用户）并重推导课题状态。
    /// - `WaitingUser` 课题恢复为可轮询状态；用户手动暂停（`Paused`）课题保持暂停；
    /// - 无 blocked 项时无副作用（普通课题每轮 User 输入都会轻量检查）。
    fn release_waiting_user(&self, ctx: &RoundContext) -> AppResult<()> {
        let Some(topic_id) = ctx.topic_id.as_ref() else {
            return Ok(());
        };
        self.assistant.topics()?.unblock_scope_items(topic_id)?;
        Ok(())
    }

    /// 推进轮（ManualStep/Poller）：课题简报刷新决策 + 选型频率。
    ///
    /// 三条件任一命中即重新生成简报（写 brief_cache + last_brief_round）；
    /// 「生成一次，落库一次」——仅刷新（生成）的这一轮由 Poller 落 nudge 输入消息，
    /// 复用缓存简报的推进轮不落重复 nudge。未命中时复用缓存简报，不重喂模型。
    fn advance_brief(&self, ctx: &mut RoundContext) -> AppResult<()> {
        let topic_id = ctx.topic_id.as_ref().ok_or_else(|| {
            AppError::InvalidInput(
                "Assistant step requires a topic bound to the session".into(),
            )
        })?;
        let topic = self.assistant.topics()?.get(topic_id)?.ok_or_else(|| {
            AppError::ConversationNotFound(topic_id.clone())
        })?;
        let state = read_assistant_state(&topic);
        let fresh = build_topic_brief(&topic);
        // 上轮若以工具调用结束，历史自带工具返回，简报非必选（可复用缓存）。
        let last_is_tool = self
            .assistant
            .runner
            .last_message_is_tool_result(&ctx.session_id)?;
        // 三条件任一命中即刷新简报：①距上次生成 ≥ BRIEF_EVERY_N_ROUNDS 轮（频率兜底）
        // ②课题有变化（fresh 与缓存不同，自动覆盖进度/scope/切换/新增）
        // ③上轮非工具调用结束（模型需课题状态锚定，屏除轮次限制）。
        let need_fresh = should_refresh_brief(
            &fresh,
            state.brief_cache.as_deref(),
            state.poll_count,
            state.last_brief_round,
            last_is_tool,
        );
        tracing::info!(
            phase = "assistant_hook",
            trigger = ?ctx.trigger,
            session_id = %ctx.session_id,
            topic_id = %topic_id,
            need_fresh,
            last_is_tool,
            poll_count = state.poll_count,
            reselect = state.poll_count % SELECTION_EVERY_N_ROUNDS == 0,
            "advance brief refresh decision"
        );
        if need_fresh {
            let mut next = state.clone();
            next.brief_cache = Some(fresh.clone());
            next.last_brief_round = state.poll_count;
            write_assistant_state(&self.assistant.topic_store, topic_id, next)?;
            ctx.model_input = fresh;
            // 「生成一次，落库一次」：仅简报刷新（生成）的这一轮落 nudge 输入消息，
            // 复用缓存简报的推进轮不落重复 nudge。
            if ctx.trigger == RoundTriggerKind::Poller {
                ctx.nudge_persist = true;
            }
        } else {
            ctx.model_input = state.brief_cache.clone().unwrap_or(fresh);
        }
        // 选型频率（业务层算好）：每 SELECTION_EVERY_N_ROUNDS 个推进轮做一次选型，
        // 中间轮沿用 last_selected 锚点；User 轮不设（默认 true，每轮选型）。
        ctx.reselect = state.poll_count % SELECTION_EVERY_N_ROUNDS == 0;
        Ok(())
    }

    /// 干预打分：对会话最后一个介入区间（上次用户介入之后到现在）调用模型打分
    /// （解析失败仅 warn + skip，不阻断主对话）。
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
        // 用户输入在 before hook 之后才落库，本次介入尚未进入消息列表；
        // 以列表末尾为锚点推导「上次介入（不含）之后」的盖章神经元。
        let neuron_ids = interval_neuron_ids(&conversation.messages, conversation.messages.len());
        if neuron_ids.is_empty() {
            tracing::info!(
                phase = "score_feedback_hook",
                topic_id = %topic_id,
                "skip: last interval has no stamped neuron"
            );
            return Ok(());
        }
        tracing::info!(
            phase = "score_feedback_hook",
            topic_id = %topic_id,
            neuron_count = neuron_ids.len(),
            "scoring last intervention interval"
        );
        // 同源：用本轮主对话同一模型（用户所选），不读配置默认。
        let model = &ctx.model;
        let decision = match self
            .assistant
            .call_judgement(
                SYSTEM_TYPE_SCORE_FEEDBACK,
                json!({
                    "user_input": ctx.model_input,
                    "topic_id": topic_id,
                    "neuron_ids": neuron_ids,
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
            .apply_score_feedback(&topic_id, neuron_ids, score as f64)
            .await
    }

    /// 课题匹配/创建/切换：模型裁决 action（switch → 已有课题；create → 新建课题）。
    async fn match_topic(&self, ctx: &mut RoundContext) -> AppResult<()> {
        // 同源：用本轮主对话同一模型（用户所选），不读配置默认。
        let model = &ctx.model;
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

    /// 课题范围修订：调用模型裁决 scope_in 增删改（add/remove/update），逐项容错落库并留痕。
    ///
    /// - 与 `complete_scope` 平行（在其之前执行）：先改内容再验收，新加项本轮即可参与勾选。
    /// - 触发类型门禁：`completed` 项仅 User 轮允许 edit/remove；ManualStep / Poller 一律跳过（记 skipped_ids）。
    /// - 空 diff 无副作用（不写留痕）；reason 缺失时用占位「（无 reason）」。
    async fn revise_topic(&self, ctx: &mut RoundContext) -> AppResult<()> {
        let Some(topic_id) = ctx.topic_id.clone() else {
            tracing::info!(phase = "revise_topic_hook", "skip: no topic");
            return Ok(());
        };
        let topic = match self.assistant.topics()?.get(&topic_id)? {
            Some(topic) => topic,
            None => {
                tracing::info!(
                    phase = "revise_topic_hook",
                    topic_id = %topic_id,
                    "skip: topic missing"
                );
                return Ok(());
            }
        };
        if topic.scope_in.is_empty() {
            tracing::info!(
                phase = "revise_topic_hook",
                topic_id = %topic_id,
                "skip: empty scope_in"
            );
            return Ok(());
        }
        // 暂停 / 等待用户课题不做变更写入（避免触发 mutate 报错）。
        if matches!(
            topic.status,
            TopicStatus::Paused | TopicStatus::WaitingUser
        ) {
            tracing::info!(
                phase = "revise_topic_hook",
                topic_id = %topic_id,
                status = ?topic.status,
                "skip: topic paused or waiting user"
            );
            return Ok(());
        }
        let outcome = ctx
            .outcome
            .as_ref()
            .ok_or_else(|| AppError::InvalidInput("revise_topic requires a finished round".into()))?;
        let model_output = outcome.model_output.clone();
        let tool_results = outcome.tool_results.clone();
        let trigger = match ctx.trigger {
            RoundTriggerKind::User => "user",
            RoundTriggerKind::ManualStep => "manual",
            RoundTriggerKind::Poller => "poller",
            RoundTriggerKind::AgentLoop => "agent_loop",
        };
        tracing::info!(
            phase = "revise_topic_hook",
            topic_id = %topic_id,
            trigger,
            scope_items = topic.scope_in.len(),
            "calling revise-topic model"
        );
        // 同源：用本轮主对话同一模型（用户所选），不读配置默认。
        let model = &ctx.model;
        let decision = self
            .assistant
            .call_judgement(
                SYSTEM_TYPE_REVISE_TOPIC,
                json!({
                    "topic_id": topic_id,
                    "scope_in": topic.scope_in,
                    "model_output": model_output,
                    "tool_results": tool_results,
                    "user_input": ctx.model_input,
                    "trigger": trigger,
                }),
                &model,
                &ctx.messages,
            )
            .await?;
        let reason = decision
            .get("reason")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("（无 reason）")
            .to_string();
        // 当前各项状态快照：completed 门禁仅 User 轮放行（Poller/ManualStep 一律跳过）。
        let is_user_round = matches!(ctx.trigger, RoundTriggerKind::User);
        let mut status_of: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        for item in &topic.scope_in {
            status_of.insert(item.id.as_str(), item.status.as_str());
        }
        let plan = parse_scope_revision(&decision, &status_of, is_user_round);
        let mut added = 0usize;
        let mut removed_ids: Vec<String> = Vec::new();
        let mut updated_ids: Vec<String> = Vec::new();
        let skipped_ids = plan.skipped_ids;
        {
            // 独立作用域：应用结束后释放 TopicStore 锁，供后续 append_revision_log 再取。
            let stores = self.assistant.topics()?;
            for (goal, contract) in &plan.add_items {
                match stores.add_scope_item(&topic_id, goal, contract) {
                    Ok(_) => added += 1,
                    Err(error) => tracing::warn!(
                        phase = "revise_topic_hook",
                        error = %error,
                        "add scope item failed"
                    ),
                }
            }
            for item_id in &plan.remove_item_ids {
                match stores.delete_scope_item(&topic_id, item_id) {
                    Ok(_) => removed_ids.push(item_id.clone()),
                    Err(error) => tracing::warn!(
                        phase = "revise_topic_hook",
                        error = %error,
                        item_id,
                        "remove scope item failed"
                    ),
                }
            }
            for (item_id, goal, contract) in &plan.update_items {
                match stores.update_scope_item(&topic_id, item_id, goal.as_deref(), contract.as_deref())
                {
                    Ok(_) => updated_ids.push(item_id.clone()),
                    Err(error) => tracing::warn!(
                        phase = "revise_topic_hook",
                        error = %error,
                        item_id,
                        "update scope item failed"
                    ),
                }
            }
        }
        // 留痕：有实际应用（add/remove/update 任一）或门禁跳过时记录；空 diff 不写。
        if added > 0 || !removed_ids.is_empty() || !updated_ids.is_empty() || !skipped_ids.is_empty() {
            let removed_len = removed_ids.len();
            let updated_len = updated_ids.len();
            let skipped_len = skipped_ids.len();
            let event = json!({
                "ts": now_ms(),
                "trigger": trigger,
                "reason": reason,
                "added": added,
                "removed_ids": removed_ids,
                "updated_ids": updated_ids,
                "skipped_ids": skipped_ids,
            });
            let _ = append_revision_log(&self.assistant.topic_store, &topic_id, event);
            tracing::info!(
                phase = "revise_topic_hook",
                topic_id = %topic_id,
                trigger,
                added,
                removed = removed_len,
                updated = updated_len,
                skipped = skipped_len,
                "revision applied"
            );
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
        // 暂停 / 等待用户课题不做裁决写入（避免触发 mutate 报错）。
        if matches!(
            topic.status,
            TopicStatus::Paused | TopicStatus::WaitingUser
        ) {
            tracing::info!(
                phase = "complete_scope_hook",
                topic_id = %topic_id,
                status = ?topic.status,
                "skip: topic paused or waiting user"
            );
            return Ok(());
        }
        // 本轮最后一条是否为工具调用结果（persist_outcome 先于 after hooks，反映本轮）。
        let last_is_tool = self
            .assistant
            .runner
            .last_message_is_tool_result(&ctx.session_id)?;
        // 收尾关闭判断（前置）：WrappingUp 课题在本轮以文本收尾（无工具调用）后关闭。
        if topic.status == TopicStatus::WrappingUp {
            if !last_is_tool {
                self.assistant
                    .topics()?
                    .set_status(&topic_id, TopicStatus::Done)?;
                tracing::info!(
                    phase = "complete_scope_hook",
                    topic_id = %topic_id,
                    "wrap-up round finished; topic closed"
                );
            }
            return Ok(());
        }
        let outcome = ctx
            .outcome
            .as_ref()
            .ok_or_else(|| AppError::InvalidInput("complete_scope requires a finished round".into()))?;
        let model_output = outcome.model_output.clone();
        let tool_results = outcome.tool_results.clone();
        tracing::info!(
            phase = "complete_scope_hook",
            topic_id = %topic_id,
            scope_items = topic.scope_in.len(),
            "calling complete-scope model"
        );
        // 同源：用本轮主对话同一模型（用户所选），不读配置默认。
        let model = &ctx.model;
        let decision = self
            .assistant
            .call_judgement(
                SYSTEM_TYPE_COMPLETE_SCOPE,
                json!({
                    "topic_id": topic_id,
                    "scope_in": topic.scope_in,
                    "model_output": model_output,
                    "tool_results": tool_results,
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
        let blocked_ids = decision
            .get("blocked_item_ids")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        tracing::info!(
            phase = "complete_scope_hook",
            completed = ids.len(),
            blocked = blocked_ids.len(),
            "updating scope items"
        );
        for id in &ids {
            let Some(item_id) = id.as_str() else {
                continue;
            };
            let _ = self
                .assistant
                .topics()?
                .complete_scope_item(&topic_id, item_id);
        }
        for id in &blocked_ids {
            let Some(item_id) = id.as_str() else {
                continue;
            };
            let _ = self
                .assistant
                .topics()?
                .mark_scope_item_blocked(&topic_id, item_id);
        }
        // 延迟关闭判断（后置）：最后一项本轮完成，但本轮以工具调用结束（模型尚未产出
        // 最终总结）→ 置 WrappingUp 保持轮询，下一轮给收尾机会，而不是直接关闭课题。
        let topic_after = match self.assistant.topics()?.get(&topic_id)? {
            Some(topic) => topic,
            None => return Ok(()),
        };
        if should_delay_close(&topic_after.status, last_is_tool) {
            self.assistant
                .topics()?
                .set_status(&topic_id, TopicStatus::WrappingUp)?;
            tracing::info!(
                phase = "complete_scope_hook",
                topic_id = %topic_id,
                "scope completed via tool round; topic wrapping up"
            );
        }
        Ok(())
    }

    /// 轮次计数递增：`total_rounds` 每成功轮 +1；User 轮 `user_rounds` +1 且 `poll_count`
    /// 归零（"距上次用户接入的推进轮次"），Manual/Poller 推进 `poll_count` +1。
    /// 仍留 topic.extra.assistant（会话运行态已迁至 conversation）。
    fn tick_round_counters(&self, ctx: &RoundContext, user_round: bool) -> AppResult<()> {
        let Some(topic_id) = ctx.topic_id.as_ref() else {
            return Ok(());
        };
        let topic = self
            .assistant
            .topics()?
            .get(topic_id)?
            .ok_or_else(|| AppError::ConversationNotFound(topic_id.clone()))?;
        let mut state = read_assistant_state(&topic);
        apply_round_counter(&mut state, user_round);
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
            let (mark, label) = match item.status.as_str() {
                "completed" => ("[x]", "验收"),
                // 等待用户介入的项：简报标记"等待用户"，模型勿选
                "blocked" => ("[⏳]", "等待用户"),
                _ => ("[ ]", "验收"),
            };
            out.push_str(&format!(
                "- {mark} {}\n    {label}：{}\n",
                item.goal.trim(),
                item.done_contract.trim()
            ));
        }
    }
    if topic.status == TopicStatus::WrappingUp {
        out.push_str(
            "本轮任务：所有事项均已完成，请输出最终总结并复核本课题的完成情况，本轮无需调用工具。",
        );
    } else {
        out.push_str(
            "本轮任务：基于上述课题，选择一件尚未完成的事项推进；必要时调用可用工具执行，并在回复中说明本轮进展。若所有事项均已完成，输出完成总结。",
        );
    }
    out
}

/// 课题简报刷新频率：每 N 个推进轮至少刷新一次（另有课题变更 / 上轮非工具结束即时刷新）。
const BRIEF_EVERY_N_ROUNDS: u64 = 3;

/// 主对话选型频率：每 N 个推进轮做一次 LLM 选型，中间轮沿用 `last_selected` 锚点
/// （业务层算好 `poll_count % N == 0` 后传 `reselect`，引擎不持有频率概念）。
const SELECTION_EVERY_N_ROUNDS: u64 = 5;

/// topic 侧助手状态：轮次计数 + 简报缓存（会话运行态已迁至 conversation.extra.session.state）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AssistantTopicState {
    /// 距上次用户接入的推进轮次（User 轮归零）；简报 3 轮 / 选型 5 轮频率的基准。
    #[serde(default)]
    poll_count: u64,
    /// 总轮次（User + ManualStep + Poller，成功跑完即计）。
    #[serde(default)]
    total_rounds: u64,
    /// 用户接入轮次。
    #[serde(default)]
    user_rounds: u64,
    /// 上份课题简报缓存（推进轮复用，避免每轮重喂长简报）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    brief_cache: Option<String>,
    /// 上次生成简报时的 poll_count（距上次 ≥ `BRIEF_EVERY_N_ROUNDS` 轮才因频率刷新）。
    #[serde(default)]
    last_brief_round: u64,
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

/// 课题修订留痕：追加一条事件到 `topic.extra.revisions` 数组（复用 `write_assistant_state`
/// 的 extra 读改写模式；事件由调用方构造，含 ts / trigger / reason / 变更明细）。
fn append_revision_log(
    topic_store: &Arc<Mutex<TopicStore>>,
    topic_id: &str,
    event: serde_json::Value,
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
    let revisions = extra
        .as_object_mut()
        .unwrap()
        .entry(String::from("revisions"))
        .or_insert_with(|| json!([]));
    if let Some(arr) = revisions.as_array_mut() {
        arr.push(event);
    }
    store.update(
        topic_id,
        TopicUpdate {
            extra: Some(Some(extra)),
            ..Default::default()
        },
    )?;
    Ok(())
}

/// Unix 毫秒时间戳（修订留痕事件时间戳；SystemTime 失败回退 0）。
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 修订计划：`revise_topic` 裁决 JSON 的解析结果（应用前纯计算，便于单测）。
#[derive(Debug, Default)]
struct RevisionPlan {
    /// 待新增项：(goal, done_contract)，已过滤空字段。
    add_items: Vec<(String, String)>,
    /// 待删除项 id。
    remove_item_ids: Vec<String>,
    /// 待编辑项：(id, goal, done_contract)（各自 trim 后非空才携带，全空仍进入计划，
    /// 由存储层 `update_scope_item` 拒绝并降级为 warn）。
    update_items: Vec<(String, Option<String>, Option<String>)>,
    /// 门禁跳过的 id（`completed` 项且非 User 轮，仅留痕不执行）。
    skipped_ids: Vec<String>,
}

/// 解析 revise 裁决 JSON 为修订计划：
/// - `add_items`：goal / done_contract 均非空才进入计划（缺一即整项丢弃）。
/// - `remove_item_ids` / `update_items`：id 必须非空；`completed` 项且非 User 轮 → 记 skipped_ids。
/// - `update_items`：goal / done_contract 各自 trim 后非空才携带。
fn parse_scope_revision(
    decision: &serde_json::Value,
    status_of: &std::collections::HashMap<&str, &str>,
    is_user_round: bool,
) -> RevisionPlan {
    let mut plan = RevisionPlan::default();
    if let Some(items) = decision.get("add_items").and_then(|v| v.as_array()) {
        for item in items {
            let goal = item
                .get("goal")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from);
            let contract = item
                .get("done_contract")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from);
            if let (Some(goal), Some(contract)) = (goal, contract) {
                plan.add_items.push((goal, contract));
            }
        }
    }
    if let Some(ids) = decision.get("remove_item_ids").and_then(|v| v.as_array()) {
        for id in ids {
            let Some(item_id) = id.as_str() else {
                continue;
            };
            if !is_user_round && status_of.get(item_id).copied() == Some("completed") {
                plan.skipped_ids.push(item_id.to_string());
            } else {
                plan.remove_item_ids.push(item_id.to_string());
            }
        }
    }
    if let Some(items) = decision.get("update_items").and_then(|v| v.as_array()) {
        for item in items {
            let Some(item_id) = item
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            if !is_user_round && status_of.get(item_id).copied() == Some("completed") {
                plan.skipped_ids.push(item_id.to_string());
                continue;
            }
            let goal = item
                .get("goal")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from);
            let contract = item
                .get("done_contract")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from);
            plan.update_items.push((item_id.to_string(), goal, contract));
        }
    }
    plan
}

/// PollAll 是否跳过该课题：`list_unfinished` 天然包含 `waiting_user`（SQL 排除仅 done/cancelled），
/// 若此处不显式跳过，等待用户介入的课题会被 Poller 每轮空转推进（边界 1）。纯函数便于单测。
fn skip_polling(status: &TopicStatus) -> bool {
    matches!(
        status,
        TopicStatus::Paused | TopicStatus::Cancelled | TopicStatus::WaitingUser
    )
}

/// 延迟关闭判断：scope 已 100% 完成但本轮以工具调用结束（模型尚未产出最终总结）→ 置
/// `WrappingUp` 保持轮询；非工具轮则存储层已推导为 `Done`。纯函数便于单测。
fn should_delay_close(status: &TopicStatus, last_is_tool: bool) -> bool {
    *status == TopicStatus::Done && last_is_tool
}

/// 简报是否需刷新：三条件任一命中即刷新（供 before_round 推进分支使用，纯函数便于单测）。
/// ①课题有变化（fresh 与缓存不同）②上轮非工具调用结束 ③距上次生成 ≥ BRIEF_EVERY_N_ROUNDS 轮。
fn should_refresh_brief(
    fresh: &str,
    cache: Option<&str>,
    poll_count: u64,
    last_brief_round: u64,
    last_is_tool: bool,
) -> bool {
    fresh != cache.unwrap_or("")
        || !last_is_tool
        || poll_count.saturating_sub(last_brief_round) >= BRIEF_EVERY_N_ROUNDS
}

/// 轮次计数语义（纯函数便于单测）：`total_rounds` 每成功轮 +1；User 轮 `user_rounds` +1 且
/// `poll_count`/`last_brief_round` 归零（"距上次用户接入的推进轮次"重新起算），
/// Manual/Poller 推进 `poll_count` +1。
fn apply_round_counter(state: &mut AssistantTopicState, user_round: bool) {
    state.total_rounds = state.total_rounds.saturating_add(1);
    if user_round {
        state.user_rounds = state.user_rounds.saturating_add(1);
        state.poll_count = 0;
        state.last_brief_round = 0;
    } else {
        state.poll_count = state.poll_count.saturating_add(1);
    }
}

/// 推导 `anchor_index` 所在介入区间的盖章神经元（去重，保留出现顺序）。
///
/// 介入边界 = `role=User` 且 `body=Text` 的消息；区间为开区间
/// `(上次介入, 下次介入)`，即上次介入（不含）之后、下次介入（不含）之前的所有消息。
/// 起点无上次介入时取 0；终点无下次介入时取 `messages.len()`。
/// 区间内消息的 `neuron_id`（`None` 跳过）去重即为可评分目标。
fn interval_neuron_ids(messages: &[Message], anchor_index: usize) -> Vec<String> {
    let is_boundary =
        |m: &Message| m.role == MessageRole::User && matches!(m.body, MessageBody::Text { .. });
    let start = (0..anchor_index)
        .rev()
        .find(|&i| is_boundary(&messages[i]))
        .map_or(0, |i| i + 1);
    let end = (anchor_index + 1..messages.len())
        .find(|&i| is_boundary(&messages[i]))
        .unwrap_or(messages.len());
    let mut seen = Vec::new();
    let mut ids = Vec::new();
    for m in &messages[start..end] {
        if let Some(id) = &m.neuron_id {
            if !seen.iter().any(|v| v == id) {
                seen.push(id.clone());
                ids.push(id.clone());
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> AssistantTopicState {
        AssistantTopicState::default()
    }

    fn msg(role: MessageRole, body: MessageBody, neuron_id: Option<&str>) -> Message {
        Message {
            role,
            body,
            timestamp: 0,
            neuron_id: neuron_id.map(String::from),
        }
    }

    fn user(text: &str) -> Message {
        msg(
            MessageRole::User,
            MessageBody::Text {
                content: text.into(),
                reasoning: None,
                tool_calls: None,
            },
            None,
        )
    }

    fn asst(text: &str, neuron: &str) -> Message {
        msg(
            MessageRole::Assistant,
            MessageBody::Text {
                content: text.into(),
                reasoning: None,
                tool_calls: None,
            },
            Some(neuron),
        )
    }

    #[test]
    fn interval_anchor_first_segment() {
        // 首段：anchor 之前无介入边界，起点取 0；终点为下一个介入边界。
        // m0 介入 / m1(n1) / m2(n1) / m3(n2) / m4 介入 / m5(n3)
        let messages = vec![
            user("q0"),
            asst("a1", "n1"),
            msg(MessageRole::Tool, MessageBody::ToolResult { tool_call_id: "t".into(), tool_name: "f".into(), content: "r".into() }, Some("n1")),
            asst("a2", "n2"),
            user("q1"),
            asst("a3", "n3"),
        ];
        // anchor=m1：区间 = m1..m4 → n1,n1,n2 → [n1, n2]
        assert_eq!(interval_neuron_ids(&messages, 1), vec!["n1".to_string(), "n2".to_string()]);
    }

    #[test]
    fn interval_anchor_middle_segment() {
        // 中段：anchor 前后均有介入边界。
        let messages = vec![
            user("q0"),
            asst("a1", "n1"),
            user("q1"),
            asst("a2", "n2"),
            asst("a3", "n2"),
            user("q2"),
            asst("a4", "n3"),
        ];
        // anchor=a2(3)：区间 = m4..m6 → n2,n2 → [n2]
        assert_eq!(interval_neuron_ids(&messages, 3), vec!["n2".to_string()]);
    }

    #[test]
    fn interval_anchor_last_segment() {
        // 末段：anchor 之后无介入边界，终点取 len。
        let messages = vec![
            user("q0"),
            asst("a1", "n1"),
            user("q1"),
            asst("a2", "n2"),
            msg(
                MessageRole::Assistant,
                MessageBody::Text {
                    content: "a3".into(),
                    reasoning: None,
                    tool_calls: None,
                },
                None,
            ),
            asst("a4", "n3"),
        ];
        // anchor=a4(5)：区间 = m3..6 → n2,None,n3 → [n2, n3]
        assert_eq!(interval_neuron_ids(&messages, 5), vec!["n2".to_string(), "n3".to_string()]);
    }

    #[test]
    fn interval_no_intervention_uses_whole_list() {
        // 无介入边界：整个消息列表即区间。
        let messages = vec![
            asst("a1", "n1"),
            asst("a2", "n2"),
            asst("a3", "n1"),
        ];
        assert_eq!(
            interval_neuron_ids(&messages, 1),
            vec!["n1".to_string(), "n2".to_string()]
        );
    }

    #[test]
    fn interval_dedup_keeps_first_seen_order() {
        // 去重：同神经元在区间内多次盖章只保留一次（保留首次出现顺序）。
        let messages = vec![
            asst("a1", "n2"),
            asst("a2", "n1"),
            asst("a3", "n2"),
            asst("a4", "n3"),
        ];
        assert_eq!(
            interval_neuron_ids(&messages, 0),
            vec!["n2".to_string(), "n1".to_string(), "n3".to_string()]
        );
    }

    #[test]
    fn interval_anchor_on_boundary_itself() {
        // anchor 为介入边界（user 消息）：上次介入严格在其前、下次介入严格在其后，区间不含自身。
        let messages = vec![
            user("q0"),
            asst("a1", "n1"),
            user("q1"),
            asst("a2", "n2"),
            user("q2"),
            asst("a3", "n3"),
        ];
        // anchor=q1(2)：区间 = m1..m4 → n1,n2 → [n1, n2]
        assert_eq!(interval_neuron_ids(&messages, 2), vec!["n1".to_string(), "n2".to_string()]);
    }

    #[test]
    fn brief_refresh_round_gap_condition() {
        // ③ 频率兜底：poll_count - last_brief_round ≥ 3 且其余条件不命中时也刷新。
        assert!(!should_refresh_brief("brief", Some("brief"), 1, 0, true));
        assert!(!should_refresh_brief("brief", Some("brief"), 2, 0, true));
        assert!(should_refresh_brief("brief", Some("brief"), 3, 0, true));
        assert!(should_refresh_brief("brief", Some("brief"), 5, 2, true));
    }

    #[test]
    fn brief_refresh_topic_changed_condition() {
        // ① 课题变化（进度/scope/切换/新增）：fresh ≠ cache 立即刷新，不受频率与工具结束限制。
        assert!(should_refresh_brief("brief-v2", Some("brief-v1"), 0, 0, true));
        assert!(should_refresh_brief("brief", None, 0, 0, true)); // 无缓存视为变化
    }

    #[test]
    fn brief_refresh_non_tool_end_condition() {
        // ② 上轮非工具调用结束：屏除轮次限制，直接给简报。
        assert!(should_refresh_brief("brief", Some("brief"), 1, 0, false));
        // 上轮工具结束 + 无变化 + 未达频率 → 复用缓存。
        assert!(!should_refresh_brief("brief", Some("brief"), 1, 0, true));
    }

    #[test]
    fn counters_user_round_resets_poll_and_increments_all() {
        // 成功跑完即计：total 每轮 +1；User 轮 user +1 且 poll_count/last_brief_round 归零。
        let mut s = state();
        s.poll_count = 2;
        s.last_brief_round = 2;
        apply_round_counter(&mut s, true);
        assert_eq!(s.total_rounds, 1);
        assert_eq!(s.user_rounds, 1);
        assert_eq!(s.poll_count, 0);
        assert_eq!(s.last_brief_round, 0);
    }

    #[test]
    fn counters_poll_round_increments_poll_only() {
        // Manual/Poller 推进：total +1，poll_count +1，user 不变。
        let mut s = state();
        s.user_rounds = 1;
        apply_round_counter(&mut s, false);
        apply_round_counter(&mut s, false);
        assert_eq!(s.total_rounds, 2);
        assert_eq!(s.user_rounds, 1);
        assert_eq!(s.poll_count, 2);
    }

    #[test]
    fn assistant_state_serde_roundtrip_with_defaults() {
        // 旧数据兼容：缺失字段回落默认；序列化时 None 缓存不输出。
        let value = serde_json::json!({"poll_count": 3});
        let s: AssistantTopicState = serde_json::from_value(value).unwrap();
        assert_eq!(s.poll_count, 3);
        assert_eq!(s.total_rounds, 0);
        assert_eq!(s.user_rounds, 0);
        assert_eq!(s.brief_cache, None);

        let roundtrip: AssistantTopicState =
            serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
        assert_eq!(roundtrip.poll_count, 3);
    }

    #[test]
    fn poll_skip_filter_excludes_waiting_user() {
        // PollAll 跳过清单：Paused / Cancelled / WaitingUser（waiting_user 必须显式排除，
        // 否则 list_unfinished 天然列出它并导致等待用户课题被轮询空转——边界 1）。
        assert!(skip_polling(&TopicStatus::Paused));
        assert!(skip_polling(&TopicStatus::Cancelled));
        assert!(skip_polling(&TopicStatus::WaitingUser));
        assert!(!skip_polling(&TopicStatus::Todo));
        assert!(!skip_polling(&TopicStatus::InProgress));
        assert!(!skip_polling(&TopicStatus::Done));
        // WrappingUp 仍需轮询（等待收尾总结）
        assert!(!skip_polling(&TopicStatus::WrappingUp));
    }

    #[test]
    fn delay_close_only_when_done_via_tool_round() {
        // 边界 2：scope 100% 完成但本轮以工具调用结束 → 延迟关闭（置 WrappingUp）。
        assert!(should_delay_close(&TopicStatus::Done, true));
        // 非工具轮（模型已产出文本收尾）→ 正常关闭为 Done。
        assert!(!should_delay_close(&TopicStatus::Done, false));
        assert!(!should_delay_close(&TopicStatus::WrappingUp, true));
        assert!(!should_delay_close(&TopicStatus::InProgress, true));
    }

    fn brief_topic(status: TopicStatus, scope: Vec<ScopeInItem>) -> Topic {
        Topic {
            id: "t1".into(),
            name: "Test".into(),
            status,
            description: String::new(),
            scope_in: scope,
            progress: 0,
            session_id: None,
            extra: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn topic_brief_marks_blocked_items_and_skips_normal_instruction() {
        // blocked 项渲染 [⏳] + "等待用户"（模型勿选）；completed 项仍 [x]。
        let topic = brief_topic(
            TopicStatus::WaitingUser,
            vec![
                ScopeInItem {
                    id: "s1".into(),
                    goal: "G1".into(),
                    done_contract: "C1".into(),
                    status: "blocked".into(),
                },
                ScopeInItem {
                    id: "s2".into(),
                    goal: "G2".into(),
                    done_contract: "C2".into(),
                    status: "completed".into(),
                },
            ],
        );
        let brief = build_topic_brief(&topic);
        assert!(brief.contains("[⏳] G1"));
        assert!(brief.contains("等待用户：C1"));
        assert!(brief.contains("[x] G2"));
        // WaitingUser 课题仍走常规推进指令（等待用户介入后由 before hook 解除）
        assert!(!brief.contains("本轮无需调用工具"));
    }

    #[test]
    fn topic_brief_wrapping_up_uses_wrapup_instruction() {
        // WrappingUp 课题：指令切换为"输出最终总结、本轮无需调用工具"。
        let wrap = build_topic_brief(&brief_topic(TopicStatus::WrappingUp, vec![]));
        assert!(wrap.contains("所有事项均已完成"));
        assert!(wrap.contains("本轮无需调用工具"));

        let normal = build_topic_brief(&brief_topic(TopicStatus::InProgress, vec![]));
        assert!(!normal.contains("本轮无需调用工具"));
    }

    fn status_map(pairs: &[(&'static str, &'static str)]) -> std::collections::HashMap<&'static str, &'static str> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn revision_parses_full_diff_with_field_filters() {
        // 完整 diff：add（含空字段丢弃）/ remove / update（含部分空字段）。
        let decision = serde_json::json!({
            "add_items": [
                {"goal": "新子目标", "done_contract": "可判定验收"},
                {"goal": "  ", "done_contract": "缺 goal 整项丢弃"},
                {"goal": "缺 contract", "done_contract": ""}
            ],
            "remove_item_ids": ["s1", "s_missing"],
            "update_items": [
                {"id": "s2", "goal": "新目标", "done_contract": "新验收"},
                {"id": "s3", "done_contract": "只改验收"}
            ],
            "reason": "用户补充需求"
        });
        let status_of = status_map(&[("s1", "pending"), ("s2", "pending"), ("s3", "pending")]);
        let plan = parse_scope_revision(&decision, &status_of, true);
        assert_eq!(plan.add_items.len(), 1);
        assert_eq!(plan.add_items[0], ("新子目标".to_string(), "可判定验收".to_string()));
        assert_eq!(plan.remove_item_ids, vec!["s1".to_string(), "s_missing".to_string()]);
        assert_eq!(plan.update_items.len(), 2);
        assert_eq!(
            plan.update_items[0],
            ("s2".to_string(), Some("新目标".to_string()), Some("新验收".to_string()))
        );
        assert_eq!(
            plan.update_items[1],
            ("s3".to_string(), None, Some("只改验收".to_string()))
        );
        assert!(plan.skipped_ids.is_empty());
    }

    #[test]
    fn revision_empty_diff_yields_empty_plan() {
        // 空 diff（无任何字段）→ 空计划，hook 层据此不产生副作用、不写留痕。
        let decision = serde_json::json!({"reason": "本轮无变更"});
        let status_of = status_map(&[]);
        let plan = parse_scope_revision(&decision, &status_of, false);
        assert!(plan.add_items.is_empty());
        assert!(plan.remove_item_ids.is_empty());
        assert!(plan.update_items.is_empty());
        assert!(plan.skipped_ids.is_empty());
    }

    #[test]
    fn revision_gate_skips_completed_items_outside_user_round() {
        // completed 门禁：Poller/ManualStep（非 User 轮）对 completed 项一律跳过（仅留痕）。
        let decision = serde_json::json!({
            "remove_item_ids": ["s_done"],
            "update_items": [{"id": "s_done", "goal": "改已完成项"}],
            "reason": "轮询轮尝试改动"
        });
        let status_of = status_map(&[("s_done", "completed")]);
        let plan = parse_scope_revision(&decision, &status_of, false);
        assert!(plan.remove_item_ids.is_empty());
        assert!(plan.update_items.is_empty());
        assert_eq!(plan.skipped_ids, vec!["s_done".to_string(), "s_done".to_string()]);

        // User 轮放行：completed 项可 edit/remove（存储层负责重置 pending）。
        let plan_user = parse_scope_revision(&decision, &status_of, true);
        assert_eq!(plan_user.remove_item_ids, vec!["s_done".to_string()]);
        assert_eq!(plan_user.update_items.len(), 1);
        assert!(plan_user.skipped_ids.is_empty());
    }

    #[test]
    fn revision_pending_items_always_editable() {
        // 非 completed 项（pending/blocked）不受门禁限制，任意轮都可改。
        let decision = serde_json::json!({
            "update_items": [{"id": "s_pending", "goal": "调整"}],
            "remove_item_ids": ["s_blocked"],
            "reason": "契约过时"
        });
        let status_of = status_map(&[("s_pending", "pending"), ("s_blocked", "blocked")]);
        let plan = parse_scope_revision(&decision, &status_of, false);
        assert_eq!(plan.update_items.len(), 1);
        assert_eq!(plan.remove_item_ids, vec!["s_blocked".to_string()]);
        assert!(plan.skipped_ids.is_empty());
    }

    #[test]
    fn revision_update_with_all_empty_fields_still_planned() {
        // update 全空字段：仍进入计划（goal/contract 均为 None），由存储层拒绝降级为 warn。
        let decision = serde_json::json!({
            "update_items": [{"id": "s1"}],
            "reason": "非法更新"
        });
        let status_of = status_map(&[("s1", "pending")]);
        let plan = parse_scope_revision(&decision, &status_of, true);
        assert_eq!(plan.update_items.len(), 1);
        assert_eq!(plan.update_items[0], ("s1".to_string(), None, None));
    }

    #[test]
    fn revision_ignores_non_string_ids_and_missing_ids() {
        // id 非字符串 / 缺失 → 该条忽略（不编造 id）。
        let decision = serde_json::json!({
            "remove_item_ids": [123, null],
            "update_items": [{"goal": "无 id"}, {}],
            "reason": "脏数据"
        });
        let status_of = status_map(&[]);
        let plan = parse_scope_revision(&decision, &status_of, true);
        assert!(plan.remove_item_ids.is_empty());
        assert!(plan.update_items.is_empty());
    }
}

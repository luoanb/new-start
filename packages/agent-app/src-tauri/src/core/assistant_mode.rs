use std::fmt;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;

use super::{
    conversation_store::{now_ms, ConversationStore},
    error::{AppError, AppResult},
    models::{
        CandidateQuery, ChatModelSelection, ChatResponse, Message, MessageRole, ModelCallRequest,
        ModelMessage, ModelMessageRole, Neuron, Topic, TopicStatus, TopicUpdate,
    },
    neuron_manager::NeuronManager,
    neuron_store::NeuronStore,
    poller::{PollHandler, Poller},
    providers::ProviderRegistry,
    tool_registry::ToolRegistry,
    topic_store::TopicStore,
};

pub const SYSTEM_TYPE_SELECT_NEURON: &str = "assistant_select_neuron";
pub const SYSTEM_TYPE_MATCH_TOPIC: &str = "assistant_match_topic";
pub const SYSTEM_TYPE_COMPLETE_SCOPE: &str = "assistant_complete_scope";
pub const SYSTEM_TYPE_SCORE_FEEDBACK: &str = "assistant_score_feedback";
pub const ASSISTANT_POLL_TASK: &str = "assistant_advance";
pub const DEFAULT_ASSISTANT_POLL_TICKS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundTrigger {
    UserInput,
    ManualStep,
    Poller,
}

#[derive(Debug, Clone)]
pub struct AssistantRoundContext {
    pub session_id: String,
    pub topic_id: Option<String>,
    pub trigger: RoundTrigger,
    pub user_input: Option<String>,
    pub system_prompt: Option<String>,
    pub selected_neuron: Option<Neuron>,
    pub authorized_tool_ids: Vec<String>,
    pub messages: Vec<ModelMessage>,
    pub model_output: Option<String>,
    pub tool_result: Option<String>,
    pub poll_count_for_topic: u64,
    pub last_selected_neuron_id: Option<String>,
    pub switched_session: bool,
}

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
    tool_registry: ToolRegistry,
    step_tx: UnboundedSender<AssistantStepRequest>,
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
        tool_registry: ToolRegistry,
        step_tx: UnboundedSender<AssistantStepRequest>,
    ) -> Self {
        Self {
            store,
            providers,
            neuron_manager,
            topic_store,
            neuron_store,
            tool_registry,
            step_tx,
        }
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
        let mut ctx = self
            .build_context(session_id, RoundTrigger::UserInput)
            .await?;
        ctx.user_input = Some(user_input.to_string());

        ScoreFeedbackBeforeHook { assistant: self }
            .run(&mut ctx)
            .await?;
        MatchTopicBeforeHook { assistant: self }
            .run(&mut ctx)
            .await?;
        SelectNeuronBeforeHook {
            assistant: self,
            secondary: false,
        }
        .run(&mut ctx)
        .await?;

        self.authorize_tools(&mut ctx);
        let response = self.run_core(&mut ctx, model).await?;
        if let Err(error) = (CompleteScopeAfterHook { assistant: self })
            .run(&mut ctx)
            .await
        {
            eprintln!("assistant afterhook failed: {error}");
            return Err(error);
        }
        self.mark_user_intervention(&ctx)?;
        Ok(response)
    }

    pub async fn step(
        &self,
        session_id: &str,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        let mut ctx = self
            .build_context(session_id, RoundTrigger::ManualStep)
            .await?;
        if ctx.topic_id.is_none() {
            return Err(AppError::InvalidInput(
                "Assistant step requires a topic bound to the session".into(),
            ));
        }
        let secondary = ctx.poll_count_for_topic >= 1;
        SelectNeuronBeforeHook {
            assistant: self,
            secondary,
        }
        .run(&mut ctx)
        .await?;
        self.authorize_tools(&mut ctx);
        let response = self.run_core(&mut ctx, model).await?;
        if let Err(error) = (CompleteScopeAfterHook { assistant: self })
            .run(&mut ctx)
            .await
        {
            eprintln!("assistant afterhook failed: {error}");
            return Err(error);
        }
        self.bump_poll_count(&ctx)?;
        Ok(response)
    }

    pub async fn step_poller(
        &self,
        session_id: &str,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        let mut ctx = self.build_context(session_id, RoundTrigger::Poller).await?;
        if ctx.topic_id.is_none() {
            return Err(AppError::InvalidInput(
                "Assistant poller step requires a topic bound to the session".into(),
            ));
        }
        let secondary = ctx.poll_count_for_topic >= 1;
        if let Err(error) = (SelectNeuronBeforeHook {
            assistant: self,
            secondary,
        })
        .run(&mut ctx)
        .await
        {
            eprintln!("assistant poller beforehook failed: {error}");
            return Err(error);
        }
        self.authorize_tools(&mut ctx);
        let response = match self.run_core(&mut ctx, model).await {
            Ok(response) => response,
            Err(error) => {
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
        &self,
        request: AssistantStepRequest,
        model: &ChatModelSelection,
    ) {
        match request {
            AssistantStepRequest::PollAll => {
                let topics = match self.topics().and_then(|store| store.list_unfinished()) {
                    Ok(topics) => topics,
                    Err(error) => {
                        eprintln!("assistant poll list failed: {error}");
                        return;
                    }
                };
                for topic in topics {
                    let Some(session_id) = topic.session_id else {
                        continue;
                    };
                    if matches!(topic.status, TopicStatus::Paused | TopicStatus::Cancelled) {
                        continue;
                    }
                    if let Err(error) = self.step_poller(&session_id, model).await {
                        eprintln!("assistant poll step failed for {}: {error}", topic.id);
                    }
                }
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
        let state = topic.as_ref().map(read_assistant_state).unwrap_or_default();
        let messages = conversation
            .messages
            .iter()
            .filter_map(message_to_model)
            .collect();
        Ok(AssistantRoundContext {
            session_id: session_id.to_string(),
            topic_id: topic.map(|t| t.id),
            trigger,
            user_input: None,
            system_prompt: None,
            selected_neuron: None,
            authorized_tool_ids: Vec::new(),
            messages,
            model_output: None,
            tool_result: None,
            poll_count_for_topic: state.poll_count,
            last_selected_neuron_id: state.last_selected_neuron_id,
            switched_session: false,
        })
    }

    fn authorize_tools(&self, ctx: &mut AssistantRoundContext) {
        let Some(neuron) = ctx.selected_neuron.as_ref() else {
            ctx.authorized_tool_ids.clear();
            return;
        };
        ctx.authorized_tool_ids = filter_authorized_tool_ids(&self.tool_registry, &neuron.tool_ids);
    }

    async fn run_core(
        &self,
        ctx: &mut AssistantRoundContext,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        let mut messages = Vec::new();
        if let Some(system_prompt) = ctx.system_prompt.clone() {
            messages.push(ModelMessage {
                role: ModelMessageRole::System,
                content: system_prompt,
                tool_calls: None,
                tool_call_id: None,
            });
        }
        messages.extend(ctx.messages.clone());
        if let Some(user_input) = ctx.user_input.clone() {
            let user_message = Message {
                role: MessageRole::User,
                content: user_input.clone(),
                timestamp: now_ms(),
                msg_type: None,
                summary_of: None,
                tool_calls: None,
                tool_call_id: None,
            };
            self.store.add_message(&ctx.session_id, user_message)?;
            messages.push(ModelMessage {
                role: ModelMessageRole::User,
                content: user_input,
                tool_calls: None,
                tool_call_id: None,
            });
        } else if matches!(ctx.trigger, RoundTrigger::ManualStep | RoundTrigger::Poller) {
            let nudge =
                "Continue advancing the bound topic using available tools if needed.".to_string();
            messages.push(ModelMessage {
                role: ModelMessageRole::User,
                content: nudge,
                tool_calls: None,
                tool_call_id: None,
            });
        }

        let tools = if ctx.authorized_tool_ids.is_empty() {
            None
        } else {
            Some(self.tool_registry.definitions_for(&ctx.authorized_tool_ids))
        };

        let model_response = self
            .providers
            .call_model(ModelCallRequest {
                provider_id: model.provider_id.clone(),
                model_id: model.model_id.clone(),
                messages: messages.clone(),
                tools,
            })
            .await?;

        let mut output = model_response.output.clone();
        let mut tool_result = None;

        if let Some(tool_calls) = model_response.tool_calls.clone() {
            if let Some(first) = tool_calls.first() {
                if !ctx.authorized_tool_ids.iter().any(|id| id == &first.name) {
                    return Err(AppError::InvalidInput(format!(
                        "Tool '{}' is not authorized for selected neuron",
                        first.name
                    )));
                }
                let result = self
                    .tool_registry
                    .execute(&first.name, first.arguments.clone())
                    .await?;
                tool_result = Some(result.clone());
                let assistant_tool_calls = Some(vec![first.clone()]);
                let tool_msg = Message {
                    role: MessageRole::Assistant,
                    content: output.clone(),
                    timestamp: now_ms(),
                    msg_type: Some("tool_call".into()),
                    summary_of: None,
                    tool_calls: assistant_tool_calls,
                    tool_call_id: None,
                };
                self.store.add_message(&ctx.session_id, tool_msg)?;
                let result_msg = Message {
                    role: MessageRole::Assistant,
                    content: result.clone(),
                    timestamp: now_ms(),
                    msg_type: Some("tool_result".into()),
                    summary_of: None,
                    tool_calls: None,
                    tool_call_id: Some(first.id.clone()),
                };
                self.store.add_message(&ctx.session_id, result_msg)?;
                output = if output.trim().is_empty() {
                    result
                } else {
                    format!("{output}\n\n[tool:{name}] {result}", name = first.name)
                };
            }
        } else {
            let assistant_msg = Message {
                role: MessageRole::Assistant,
                content: output.clone(),
                timestamp: now_ms(),
                msg_type: None,
                summary_of: None,
                tool_calls: None,
                tool_call_id: None,
            };
            self.store.add_message(&ctx.session_id, assistant_msg)?;
        }

        ctx.model_output = Some(output.clone());
        ctx.tool_result = tool_result;
        self.persist_selected_neuron(ctx)?;

        Ok(ChatResponse {
            conversation_id: ctx.session_id.clone(),
            response: output,
        })
    }

    async fn call_system_prompt_json(
        &self,
        system_type: &str,
        user_payload: serde_json::Value,
        model: &ChatModelSelection,
    ) -> AppResult<serde_json::Value> {
        let prompt_neuron = self
            .neuron_manager
            .ensure_system_neuron(system_type, false)
            .await?;
        let response = self
            .providers
            .call_model(ModelCallRequest {
                provider_id: model.provider_id.clone(),
                model_id: model.model_id.clone(),
                messages: vec![
                    ModelMessage {
                        role: ModelMessageRole::System,
                        content: prompt_neuron.content,
                        tool_calls: None,
                        tool_call_id: None,
                    },
                    ModelMessage {
                        role: ModelMessageRole::User,
                        content: user_payload.to_string(),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                ],
                tools: None,
            })
            .await?;
        extract_json_object(&response.output)
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

    fn persist_selected_neuron(&self, ctx: &AssistantRoundContext) -> AppResult<()> {
        let Some(topic_id) = ctx.topic_id.as_ref() else {
            return Ok(());
        };
        let Some(neuron) = ctx.selected_neuron.as_ref() else {
            return Ok(());
        };
        let topic = self
            .topics()?
            .get(topic_id)?
            .ok_or_else(|| AppError::ConversationNotFound(topic_id.clone()))?;
        let mut state = read_assistant_state(&topic);
        state.last_selected_neuron_id = Some(neuron.id.clone());
        if !state
            .intervention_neuron_ids
            .iter()
            .any(|id| id == &neuron.id)
        {
            state.intervention_neuron_ids.push(neuron.id.clone());
        }
        write_assistant_state(&self.topic_store, topic_id, state)
    }

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
        if let Some(neuron) = ctx.selected_neuron.as_ref() {
            state.last_selected_neuron_id = Some(neuron.id.clone());
        }
        write_assistant_state(&self.topic_store, topic_id, state)
    }

    fn mark_user_intervention(&self, ctx: &AssistantRoundContext) -> AppResult<()> {
        let Some(topic_id) = ctx.topic_id.as_ref() else {
            return Ok(());
        };
        let topic = self
            .topics()?
            .get(topic_id)?
            .ok_or_else(|| AppError::ConversationNotFound(topic_id.clone()))?;
        let mut state = read_assistant_state(&topic);
        state.last_intervention_at = Some(now_ms());
        state.intervention_neuron_ids.clear();
        if let Some(neuron) = ctx.selected_neuron.as_ref() {
            state.intervention_neuron_ids.push(neuron.id.clone());
            state.last_selected_neuron_id = Some(neuron.id.clone());
        }
        write_assistant_state(&self.topic_store, topic_id, state)
    }

    fn default_model_or_error(&self) -> AppResult<ChatModelSelection> {
        self.providers
            .default_model_selection()?
            .ok_or(AppError::ModelNotSelected)
    }
}

struct AssistantPollHandler {
    tx: UnboundedSender<AssistantStepRequest>,
}

impl PollHandler for AssistantPollHandler {
    fn on_tick(&mut self) {
        let _ = self.tx.send(AssistantStepRequest::PollAll);
    }
}

struct SelectNeuronBeforeHook<'a> {
    assistant: &'a AssistantMode,
    secondary: bool,
}

#[async_trait]
impl BeforeHook for SelectNeuronBeforeHook<'_> {
    async fn run(&self, ctx: &mut AssistantRoundContext) -> AppResult<()> {
        let source_id = if self.secondary {
            ctx.last_selected_neuron_id.clone()
        } else {
            None
        };
        let selected = self
            .assistant
            .neuron_manager
            .select_one(CandidateQuery {
                n: 7,
                source_id,
                min_new: 0,
            })
            .await?;
        ctx.system_prompt = Some(selected.content.clone());
        ctx.selected_neuron = Some(selected);
        Ok(())
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
        let decision = self
            .assistant
            .call_system_prompt_json(
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
                    })).collect::<Vec<_>>(),
                }),
                &model,
            )
            .await?;

        let action = decision
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("create");
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
                        let created = self.create_bound_topic(ctx)?;
                        ctx.topic_id = Some(created.id);
                        return Ok(());
                    }
                };
                if let Some(bound_session) = topic.session_id.clone() {
                    if bound_session != ctx.session_id {
                        ctx.session_id = bound_session;
                        ctx.switched_session = true;
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
                    let created = self.create_bound_topic(ctx)?;
                    ctx.topic_id = Some(created.id);
                }
            }
        }
        Ok(())
    }
}

impl MatchTopicBeforeHook<'_> {
    fn create_bound_topic(&self, ctx: &AssistantRoundContext) -> AppResult<Topic> {
        let name = ctx
            .user_input
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
            .unwrap_or_else(|| "Assistant Topic".to_string());
        let created = self.assistant.topics()?.create(
            &name,
            ctx.user_input.as_deref().unwrap_or(""),
            TopicStatus::Todo,
            Vec::new(),
            None,
        )?;
        self.assistant
            .topics()?
            .bind_session(&created.id, &ctx.session_id)
    }
}

struct ScoreFeedbackBeforeHook<'a> {
    assistant: &'a AssistantMode,
}

#[async_trait]
impl BeforeHook for ScoreFeedbackBeforeHook<'_> {
    async fn run(&self, ctx: &mut AssistantRoundContext) -> AppResult<()> {
        let Some(topic_id) = ctx.topic_id.clone() else {
            return Ok(());
        };
        let topic = match self.assistant.topics()?.get(&topic_id)? {
            Some(topic) => topic,
            None => return Ok(()),
        };
        let state = read_assistant_state(&topic);
        if state.last_intervention_at.is_none() || state.intervention_neuron_ids.is_empty() {
            return Ok(());
        }
        let model = self.assistant.default_model_or_error()?;
        let decision = self
            .assistant
            .call_system_prompt_json(
                SYSTEM_TYPE_SCORE_FEEDBACK,
                json!({
                    "user_input": ctx.user_input,
                    "topic_id": topic_id,
                    "neuron_ids": state.intervention_neuron_ids,
                }),
                &model,
            )
            .await?;
        let score = decision
            .get("score")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| AppError::InvalidInput("score feedback missing score".into()))?;
        if score == 0 || !(-5..=5).contains(&score) {
            return Err(AppError::InvalidInput(format!(
                "score must be in -5..=5 and non-zero, got {score}"
            )));
        }
        let delta = score as f64;
        for neuron_id in &state.intervention_neuron_ids {
            let _ = self
                .assistant
                .neuron_manager
                .adjust_weight(neuron_id, delta)?;
            let connections = self.assistant.neurons()?.get_connections(neuron_id)?;
            for edge in connections {
                if edge.target == *neuron_id || edge.source == *neuron_id {
                    let _ = self.assistant.neurons()?.adjust_connection_weight(
                        &edge.source,
                        &edge.target,
                        delta,
                    );
                }
            }
        }
        Ok(())
    }
}

struct CompleteScopeAfterHook<'a> {
    assistant: &'a AssistantMode,
}

#[async_trait]
impl AfterHook for CompleteScopeAfterHook<'_> {
    async fn run(&self, ctx: &mut AssistantRoundContext) -> AppResult<()> {
        let Some(topic_id) = ctx.topic_id.clone() else {
            return Ok(());
        };
        let topic = match self.assistant.topics()?.get(&topic_id)? {
            Some(topic) => topic,
            None => return Ok(()),
        };
        if topic.scope_in.is_empty() {
            return Ok(());
        }
        let model = self.assistant.default_model_or_error()?;
        let decision = self
            .assistant
            .call_system_prompt_json(
                SYSTEM_TYPE_COMPLETE_SCOPE,
                json!({
                    "topic_id": topic_id,
                    "scope_in": topic.scope_in,
                    "model_output": ctx.model_output,
                    "tool_result": ctx.tool_result,
                    "user_input": ctx.user_input,
                }),
                &model,
            )
            .await?;
        let ids = decision
            .get("completed_item_ids")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AssistantTopicState {
    #[serde(default)]
    poll_count: u64,
    #[serde(default)]
    last_selected_neuron_id: Option<String>,
    #[serde(default)]
    last_intervention_at: Option<u128>,
    #[serde(default)]
    intervention_neuron_ids: Vec<String>,
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

pub fn filter_authorized_tool_ids(registry: &ToolRegistry, tool_ids: &[String]) -> Vec<String> {
    let known: std::collections::HashSet<String> = registry
        .list_definitions()
        .into_iter()
        .map(|d| d.name)
        .collect();
    let mut out = Vec::new();
    for id in tool_ids {
        if known.contains(id) {
            out.push(id.clone());
        } else {
            eprintln!("assistant ignoring unknown tool id: {id}");
        }
    }
    out
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

fn message_to_model(message: &Message) -> Option<ModelMessage> {
    let role = match message.role {
        MessageRole::User => ModelMessageRole::User,
        MessageRole::Assistant => ModelMessageRole::Assistant,
        MessageRole::System => ModelMessageRole::System,
        MessageRole::Compaction => return None,
    };
    Some(ModelMessage {
        role,
        content: message.content.clone(),
        tool_calls: message.tool_calls.clone(),
        tool_call_id: message.tool_call_id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tool_registry::ToolRegistry;

    #[test]
    fn filter_drops_unknown_tool_ids() {
        let registry = ToolRegistry::with_defaults();
        let filtered = filter_authorized_tool_ids(
            &registry,
            &["echo".into(), "missing_tool".into(), "calculate".into()],
        );
        assert_eq!(filtered, vec!["echo".to_string(), "calculate".to_string()]);
    }

    #[test]
    fn extract_json_from_fenced_text() {
        let value = extract_json_object("here\n```json\n{\"score\": 2}\n```").unwrap();
        assert_eq!(value["score"], 2);
    }
}

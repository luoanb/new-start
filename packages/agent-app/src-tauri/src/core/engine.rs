use std::collections::HashSet;

use super::{
    compactor::Compactor,
    conversation_store::{now_ms, ConversationStore},
    error::{AppError, AppResult},
    models::{
        ChatModelSelection, ChatOptions, ChatResponse, CompactionConfig, ConversationMode, Message,
        MessageRole, ModelCallRequest, ModelMessage, ModelMessageRole,
    },
    providers::ProviderRegistry,
    tool_registry::ToolRegistry,
};

const AGENT_MAX_ITERATIONS: u32 = 20;

/// Engine orchestrates the chat flow: get conversation → compact → call model → save result.
#[derive(Debug, Clone)]
pub struct Engine {
    store: ConversationStore,
    providers: ProviderRegistry,
    compactor: Compactor,
    tool_registry: Option<ToolRegistry>,
}

impl Engine {
    pub fn new(
        store: ConversationStore,
        providers: ProviderRegistry,
        config: CompactionConfig,
    ) -> Self {
        Self {
            store,
            providers,
            compactor: Compactor::new(config),
            tool_registry: None,
        }
    }

    pub fn with_tools(
        store: ConversationStore,
        providers: ProviderRegistry,
        config: CompactionConfig,
        tool_registry: ToolRegistry,
    ) -> Self {
        Self {
            store,
            providers,
            compactor: Compactor::new(config),
            tool_registry: Some(tool_registry),
        }
    }

    /// Unified chat orchestration.
    ///
    /// Dispatches by `conversation.mode`:
    /// - Chat  → single model call, no tools
    /// - Agent → tool-calling loop
    pub async fn chat(
        &self,
        input: &str,
        conversation_id: String,
        options: ChatOptions,
    ) -> AppResult<ChatResponse> {
        // ── Common preamble ──────────────────────────────────────
        let mut conversation = self.store.require_conversation(&conversation_id)?;

        let context_window = self
            .providers
            .list_models(Some(&options.provider_id))
            .ok()
            .and_then(|models| {
                models
                    .iter()
                    .find(|m| m.id == options.model_id)
                    .and_then(|m| m.context_window)
            })
            .unwrap_or(128_000);

        let model = ChatModelSelection {
            provider_id: options.provider_id.clone(),
            model_id: options.model_id.clone(),
        };

        let compacted = self
            .compactor
            .ensure_fits(&mut conversation, &self.providers, &model, context_window)
            .await?;

        // Build model context (mode-aware filtering)
        let mut context_messages = build_context(&conversation, &conversation.mode);

        // Save user message (separate timestamp from model response)
        let user_ts = now_ms();
        let user_message = Message {
            role: MessageRole::User,
            content: input.to_string(),
            timestamp: user_ts,
            msg_type: None,
            summary_of: None,
            tool_calls: None,
            tool_call_id: None,
        };
        self.store.add_message(&conversation_id, user_message)?;

        // Add user input to context
        context_messages.push(ModelMessage {
            role: ModelMessageRole::User,
            content: input.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });

        // ── Dispatch by mode ────────────────────────────────────
        let response = match conversation.mode {
            ConversationMode::Chat => {
                self.chat_mode(context_messages, &options, &conversation_id)
                    .await?
            }
            ConversationMode::Agent => {
                self.agent_mode(context_messages, &options, &conversation_id)
                    .await?
            }
            ConversationMode::Assistant => {
                return Err(AppError::RuntimeError(
                    "Assistant mode must be routed via Gateway/AssistantMode".into(),
                ));
            }
        };

        // Save if compaction modified messages in-place
        if compacted {
            self.store.save_conversation(&conversation)?;
        }

        Ok(response)
    }

    /// Chat mode: single model call (no tools), save assistant response.
    async fn chat_mode(
        &self,
        messages: Vec<ModelMessage>,
        options: &ChatOptions,
        conversation_id: &str,
    ) -> AppResult<ChatResponse> {
        let model_response = self
            .providers
            .call_model(ModelCallRequest {
                provider_id: options.provider_id.clone(),
                model_id: options.model_id.clone(),
                messages,
                tools: None,
            })
            .await?;

        let assistant_msg = Message {
            role: MessageRole::Assistant,
            content: model_response.output.clone(),
            timestamp: now_ms(),
            msg_type: None,
            summary_of: None,
            tool_calls: None,
            tool_call_id: None,
        };
        self.store.add_message(conversation_id, assistant_msg)?;

        Ok(ChatResponse {
            conversation_id: conversation_id.to_string(),
            response: model_response.output,
        })
    }

    /// Agent mode: tool-calling loop.
    async fn agent_mode(
        &self,
        mut context_messages: Vec<ModelMessage>,
        options: &ChatOptions,
        conversation_id: &str,
    ) -> AppResult<ChatResponse> {
        let tool_defs = self
            .tool_registry
            .as_ref()
            .map(|reg| reg.list_definitions());

        let mut iterations = 0u32;

        loop {
            iterations += 1;
            if iterations > AGENT_MAX_ITERATIONS {
                return Err(AppError::AgentMaxIterations(format!(
                    "Agent exceeded max iterations ({})",
                    AGENT_MAX_ITERATIONS
                )));
            }

            let model_response = self
                .providers
                .call_model(ModelCallRequest {
                    provider_id: options.provider_id.clone(),
                    model_id: options.model_id.clone(),
                    messages: context_messages.clone(),
                    tools: tool_defs.clone(),
                })
                .await?;

            let has_tool_calls = model_response.finish_reason == "tool_calls"
                && model_response
                    .tool_calls
                    .as_ref()
                    .is_some_and(|c| !c.is_empty());

            if has_tool_calls {
                // Save assistant message with tool_calls
                let assistant_msg = Message {
                    role: MessageRole::Assistant,
                    content: model_response.output.clone(),
                    timestamp: now_ms(),
                    msg_type: None,
                    summary_of: None,
                    tool_calls: model_response.tool_calls.clone(),
                    tool_call_id: None,
                };
                self.store.add_message(conversation_id, assistant_msg)?;

                // Add assistant response to context
                context_messages.push(ModelMessage {
                    role: ModelMessageRole::Assistant,
                    content: model_response.output.clone(),
                    tool_calls: model_response.tool_calls.clone(),
                    tool_call_id: None,
                });

                // Execute each tool call
                let tcs = model_response.tool_calls.unwrap_or_default();
                for tc in &tcs {
                    let result = match &self.tool_registry {
                        Some(reg) => reg.execute(&tc.name, tc.arguments.clone()).await,
                        None => Err(AppError::SkillNotFound(tc.name.clone())),
                    };
                    let result_str = result.unwrap_or_else(|e| format!("Error: {}", e));

                    context_messages.push(ModelMessage {
                        role: ModelMessageRole::Tool,
                        content: result_str.clone(),
                        tool_calls: None,
                        tool_call_id: Some(tc.id.clone()),
                    });

                    let tool_result_msg = Message {
                        role: MessageRole::Assistant,
                        content: format!("[Tool {} result]: {}", tc.name, result_str),
                        timestamp: now_ms(),
                        msg_type: Some("tool_result".to_string()),
                        summary_of: None,
                        tool_calls: None,
                        tool_call_id: Some(tc.id.clone()),
                    };
                    self.store.add_message(conversation_id, tool_result_msg)?;
                }
                // Continue loop
            } else {
                // Normal text response
                let assistant_msg = Message {
                    role: MessageRole::Assistant,
                    content: model_response.output.clone(),
                    timestamp: now_ms(),
                    msg_type: None,
                    summary_of: None,
                    tool_calls: None,
                    tool_call_id: None,
                };
                self.store.add_message(conversation_id, assistant_msg)?;

                return Ok(ChatResponse {
                    conversation_id: conversation_id.to_string(),
                    response: model_response.output,
                });
            }
        }
    }

    /// Manually trigger compaction for a conversation.
    pub async fn compact(
        &self,
        conversation_id: &str,
        model: &ChatModelSelection,
    ) -> AppResult<bool> {
        let mut conversation = self.store.require_conversation(conversation_id)?;
        let result = self
            .compactor
            .compact(&mut conversation, &self.providers, model)
            .await?;
        if result {
            self.store.save_conversation(&conversation)?;
        }
        Ok(result)
    }
}

/// Build model context from conversation history, filtering by mode.
///
/// - Chat  mode: skip messages with tool_calls or tool_call_id
/// - Agent mode: include all messages (tool_calls / tool results preserved)
fn build_context(
    conversation: &super::models::Conversation,
    mode: &ConversationMode,
) -> Vec<ModelMessage> {
    let summarized: HashSet<String> = conversation
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::Compaction)
        .filter_map(|m| m.summary_of.clone())
        .flatten()
        .collect();

    conversation
        .messages
        .iter()
        .filter(|m| {
            // Always include compaction summaries
            if m.role == MessageRole::Compaction {
                return true;
            }
            // Skip messages already covered by a summary
            if summarized.contains(&m.timestamp.to_string()) {
                return false;
            }
            // Chat mode: skip tool-related messages
            if *mode == ConversationMode::Chat {
                m.tool_calls.is_none() && m.tool_call_id.is_none()
            } else {
                true
            }
        })
        .map(message_to_model_message)
        .collect()
}

/// Convert a stored Message to a ModelMessage for the LLM API call.
///
/// - Compaction messages → System role with summary prefix.
/// - Assistant messages with `tool_call_id` (tool results persisted as assistant) → Tool role.
/// - Other Assistant messages → Assistant role (preserving tool_calls if any).
fn message_to_model_message(message: &Message) -> ModelMessage {
    match message.role {
        MessageRole::System => ModelMessage {
            role: ModelMessageRole::System,
            content: message.content.clone(),
            tool_calls: None,
            tool_call_id: None,
        },
        MessageRole::User => ModelMessage {
            role: ModelMessageRole::User,
            content: message.content.clone(),
            tool_calls: None,
            tool_call_id: None,
        },
        MessageRole::Assistant => {
            if message.tool_call_id.is_some() {
                ModelMessage {
                    role: ModelMessageRole::Tool,
                    content: message.content.clone(),
                    tool_calls: None,
                    tool_call_id: message.tool_call_id.clone(),
                }
            } else {
                ModelMessage {
                    role: ModelMessageRole::Assistant,
                    content: message.content.clone(),
                    tool_calls: message.tool_calls.clone(),
                    tool_call_id: None,
                }
            }
        }
        MessageRole::Compaction => ModelMessage {
            role: ModelMessageRole::System,
            content: format!("[Previous conversation summary]: {}", message.content),
            tool_calls: None,
            tool_call_id: None,
        },
    }
}

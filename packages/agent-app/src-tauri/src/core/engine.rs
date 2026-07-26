use std::collections::HashSet;

use super::{
    compactor::Compactor,
    conversation_store::{now_ms, ConversationStore},
    error::{AppError, AppResult},
    models::{
        ChatModelSelection, ChatOptions, ChatResponse, CompactionConfig, Message, MessageRole,
        ModelCallRequest, ModelMessage, ModelMessageRole,
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

    /// Main chat orchestration.
    ///
    /// 1. Load the conversation
    /// 2. Determine model's context_window
    /// 3. Auto-compact if needed
    /// 4. Build request context from conversation messages + input
    /// 5. Call the model
    /// 6. Save user message + assistant response
    /// 7. Return response
    pub async fn chat(
        &mut self,
        input: &str,
        conversation_id: String,
        options: ChatOptions,
    ) -> AppResult<ChatResponse> {
        // 1. Load conversation
        let mut conversation = self.store.require_conversation(&conversation_id)?;

        // 2. Determine context_window
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

        // 3. Auto-compact if needed
        let compacted = self
            .compactor
            .ensure_fits(&mut conversation, &self.providers, &model, context_window)
            .await?;

        // 4. Build model request context
        // Collect timestamps of messages that have been summarized by previous compactions
        let summarized: HashSet<String> = conversation
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::Compaction)
            .filter_map(|m| m.summary_of.clone())
            .flatten()
            .collect();

        // Build context: include compaction summaries + messages NOT covered by any summary
        let mut messages: Vec<ModelMessage> = conversation
            .messages
            .iter()
            .filter(|m| {
                // Always include compaction messages (they provide context)
                if m.role == MessageRole::Compaction {
                    return true;
                }
                // Skip messages whose timestamps are recorded in any summary_of
                !summarized.contains(&m.timestamp.to_string())
            })
            .map(message_to_model_message)
            .collect();

        // Add the user input
        messages.push(ModelMessage {
            role: ModelMessageRole::User,
            content: input.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });

        // 5. Save user message before calling model (separate timestamp)
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

        // 6. Call the model
        let model_response = self
            .providers
            .call_model(ModelCallRequest {
                provider_id: options.provider_id,
                model_id: options.model_id,
                messages,
                tools: None,
            })
            .await?;

        // 7. Save assistant response (different timestamp from user message)
        let assistant_message = Message {
            role: MessageRole::Assistant,
            content: model_response.output.clone(),
            timestamp: now_ms(),
            msg_type: None,
            summary_of: None,
            tool_calls: None,
            tool_call_id: None,
        };
        self.store
            .add_message(&conversation_id, assistant_message)?;

        // If compaction happened, save the conversation state (messages have been modified in place)
        if compacted {
            self.store
                .save_conversation(&conversation)?;
        }

        // 7. Return response
        Ok(ChatResponse {
            conversation_id,
            response: model_response.output,
        })
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

    /// Agent chat with tool-calling loop.
    ///
    /// 1. Load conversation → auto-compact → build context
    /// 2. Save user message (separate timestamp)
    /// 3. Loop: call model with tools, execute tool calls, add results to context
    /// 4. When model responds with text ("stop"), save and return
    pub async fn chat_with_tools(
        &mut self,
        input: &str,
        conversation_id: String,
        options: ChatOptions,
    ) -> AppResult<ChatResponse> {
        // 1. Load conversation
        let mut conversation = self.store.require_conversation(&conversation_id)?;

        // 2. Determine context_window
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

        // 3. Auto-compact if needed
        let compacted = self
            .compactor
            .ensure_fits(&mut conversation, &self.providers, &model, context_window)
            .await?;

        // 4. Build context from history (filter summarized messages)
        let summarized: HashSet<String> = conversation
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::Compaction)
            .filter_map(|m| m.summary_of.clone())
            .flatten()
            .collect();

        let mut context_messages: Vec<ModelMessage> = conversation
            .messages
            .iter()
            .filter(|m| {
                if m.role == MessageRole::Compaction {
                    return true;
                }
                !summarized.contains(&m.timestamp.to_string())
            })
            .map(message_to_model_message)
            .collect();

        // 5. Save user message before tool loop (separate timestamp)
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

        // 6. Get tool definitions (if any)
        let tool_defs = self
            .tool_registry
            .as_ref()
            .map(|reg| reg.list_definitions());

        // 7. Tool loop
        let mut iterations = 0u32;
        let mut final_output = String::new();

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
                self.store
                    .add_message(&conversation_id, assistant_msg)?;

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

                    // Add tool result to model context (Tool role)
                    context_messages.push(ModelMessage {
                        role: ModelMessageRole::Tool,
                        content: result_str.clone(),
                        tool_calls: None,
                        tool_call_id: Some(tc.id.clone()),
                    });

                    // Persist tool result (stored as Assistant with tool_call_id)
                    let tool_result_msg = Message {
                        role: MessageRole::Assistant,
                        content: format!("[Tool {} result]: {}", tc.name, result_str),
                        timestamp: now_ms(),
                        msg_type: Some("tool_result".to_string()),
                        summary_of: None,
                        tool_calls: None,
                        tool_call_id: Some(tc.id.clone()),
                    };
                    self.store
                        .add_message(&conversation_id, tool_result_msg)?;
                }
                // Continue loop — model will see tool results and produce next response
            } else {
                // Normal text response
                final_output = model_response.output.clone();

                let assistant_msg = Message {
                    role: MessageRole::Assistant,
                    content: final_output.clone(),
                    timestamp: now_ms(),
                    msg_type: None,
                    summary_of: None,
                    tool_calls: None,
                    tool_call_id: None,
                };
                self.store
                    .add_message(&conversation_id, assistant_msg)?;

                break;
            }
        }

        // Save if compaction modified messages
        if compacted {
            self.store.save_conversation(&conversation)?;
        }

        Ok(ChatResponse {
            conversation_id,
            response: final_output,
        })
    }
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
            // If stored with tool_call_id, it's a tool result → convert to Tool role
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

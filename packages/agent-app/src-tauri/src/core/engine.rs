use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use super::{
    compactor::Compactor,
    conversation_store::{now_ms, ConversationStore},
    error::{AppError, AppResult},
    model_call_input::{ModelAppendTemplate, ModelCallInput},
    models::{
        ChatModelSelection, ChatOptions, ChatResponse, CompactionConfig, ConversationMode, Message,
        MessageBody, MessageRole, ModelCallRequest, ModelMessage, ModelMessageRole,
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
    /// 共享工具注册表（与 Gateway 同一 `Arc<RwLock>`）：读锁 clone 后立即释放，
    /// 不跨 await；运行期重装配写锁一次性替换。
    tool_registry: Option<Arc<RwLock<ToolRegistry>>>,
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
        tool_registry: Arc<RwLock<ToolRegistry>>,
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

        // Build model context (mode-aware filtering); user turn assembled via ModelCallInput.
        let context_messages = build_context(&conversation, &conversation.mode);
        let role_system = context_messages
            .iter()
            .find(|m| m.role == ModelMessageRole::System)
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let context_messages = ModelCallInput::assemble(
            &context_messages,
            &role_system,
            "",
            input,
            ModelAppendTemplate::Neuron,
        );

        // Save user message (separate timestamp from model response)
        let user_ts = now_ms();
        let user_message = Message {
            role: MessageRole::User,
            body: MessageBody::Text {
                content: input.to_string(),
            },
            timestamp: user_ts,
        };
        self.store.add_message(&conversation_id, user_message)?;

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
            body: MessageBody::Text {
                content: model_response.output.clone(),
            },
            timestamp: now_ms(),
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
        let tool_defs = self.tool_registry.as_ref().map(|reg| {
            // 读锁仅用于 clone definitions，释放后再进入模型调用 await。
            reg.read()
                .map(|g| g.list_definitions())
                .unwrap_or_default()
        });

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
                    body: MessageBody::ToolCall {
                        content: model_response.output.clone(),
                        tool_calls: model_response.tool_calls.clone().unwrap_or_default(),
                    },
                    timestamp: now_ms(),
                };
                self.store.add_message(conversation_id, assistant_msg)?;

                // Add assistant response to context
                context_messages = ModelCallInput::append(
                    &context_messages,
                    ModelMessage {
                        role: ModelMessageRole::Assistant,
                        content: model_response.output.clone(),
                        tool_calls: model_response.tool_calls.clone(),
                        tool_call_id: None,
                    },
                );

                // Execute each tool call
                let tcs = model_response.tool_calls.unwrap_or_default();
                for tc in &tcs {
                    // 读锁内仅 clone 工具引用（释放锁后再 await execute，锁不跨 await）。
                    let tool = self.tool_registry.as_ref().and_then(|reg| {
                        reg.read().ok().and_then(|g| g.get_tool(&tc.name))
                    });
                    let result = match tool {
                        Some(tool) => tool.execute(tc.arguments.clone()).await,
                        None => Err(AppError::SkillNotFound(tc.name.clone())),
                    };
                    let result_str = result.unwrap_or_else(|e| format!("Error: {}", e));

                    context_messages = ModelCallInput::append(
                        &context_messages,
                        ModelMessage {
                            role: ModelMessageRole::Tool,
                            content: result_str.clone(),
                            tool_calls: None,
                            tool_call_id: Some(tc.id.clone()),
                        },
                    );

                    let tool_result_msg = Message {
                        role: MessageRole::Tool,
                        body: MessageBody::ToolResult {
                            tool_call_id: tc.id.clone(),
                            tool_name: tc.name.clone(),
                            content: result_str,
                        },
                        timestamp: now_ms(),
                    };
                    self.store.add_message(conversation_id, tool_result_msg)?;
                }
                // Continue loop
            } else {
                // Normal text response
                let assistant_msg = Message {
                    role: MessageRole::Assistant,
                    body: MessageBody::Text {
                        content: model_response.output.clone(),
                    },
                    timestamp: now_ms(),
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
        .filter_map(|m| m.summary_of().map(|s| s.to_vec()))
        .flatten()
        .collect();

    let messages = conversation
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
                !m.is_tool()
            } else {
                true
            }
        })
        .map(message_to_model_message)
        .collect::<Vec<_>>();
    // 兜底：清理历史里可能存在的孤立 tool_calls（无对应 tool 响应），避免 400。
    ModelCallInput::sanitize_tool_pairs(&messages)
}

/// Convert a stored Message to a ModelMessage for the LLM API call.
///
/// - Compaction body → System role with summary prefix.
/// - ToolResult body → Tool role with `tool_call_id`.
/// - ToolCall body → Assistant role preserving tool_calls.
/// - Text body → mapped by the stored author role.
fn message_to_model_message(message: &Message) -> ModelMessage {
    match &message.body {
        MessageBody::Compaction { content, .. } => ModelMessage {
            role: ModelMessageRole::System,
            content: format!("[Previous conversation summary]: {content}"),
            tool_calls: None,
            tool_call_id: None,
        },
        MessageBody::ToolResult {
            tool_call_id, content, ..
        } => ModelMessage {
            role: ModelMessageRole::Tool,
            content: content.clone(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.clone()),
        },
        MessageBody::ToolCall { content, tool_calls } => ModelMessage {
            role: ModelMessageRole::Assistant,
            content: content.clone(),
            tool_calls: Some(tool_calls.clone()),
            tool_call_id: None,
        },
        MessageBody::Text { content } => {
            let role = match message.role {
                MessageRole::System => ModelMessageRole::System,
                MessageRole::User => ModelMessageRole::User,
                // Tool 角色不会携带 Text 正文（Tool 只对应 ToolResult），兜底按 Assistant 发送。
                MessageRole::Assistant | MessageRole::Tool => ModelMessageRole::Assistant,
                MessageRole::Compaction => unreachable!("handled above"),
            };
            ModelMessage {
                role,
                content: content.clone(),
                tool_calls: None,
                tool_call_id: None,
            }
        }
        // nudge 仅由 Assistant 会话轮询产生（role=User），engine（Chat/Agent）会话不会实际遇到；
        // 兜底按 User 文本处理以保持穷尽性。
        MessageBody::Nudge { content } => ModelMessage {
            role: ModelMessageRole::User,
            content: content.clone(),
            tool_calls: None,
            tool_call_id: None,
        },
    }
}

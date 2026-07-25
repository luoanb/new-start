use std::collections::HashSet;

use super::{
    compactor::Compactor,
    conversation_store::{now_ms, ConversationStore},
    error::AppResult,
    models::{
        ChatModelSelection, ChatOptions, ChatResponse, CompactionConfig, Message, MessageRole,
        ModelCallRequest, ModelMessage, ModelMessageRole,
    },
    providers::ProviderRegistry,
};

/// Engine orchestrates the chat flow: get conversation → compact → call model → save result.
#[derive(Debug, Clone)]
pub struct Engine {
    store: ConversationStore,
    providers: ProviderRegistry,
    compactor: Compactor,
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
        });

        // 5. Save user message before calling model (separate timestamp)
        let user_ts = now_ms();
        let user_message = Message {
            role: MessageRole::User,
            content: input.to_string(),
            timestamp: user_ts,
            msg_type: None,
            summary_of: None,
        };
        self.store.add_message(&conversation_id, user_message)?;

        // 6. Call the model
        let model_response = self
            .providers
            .call_model(ModelCallRequest {
                provider_id: options.provider_id,
                model_id: options.model_id,
                messages,
            })
            .await?;

        // 7. Save assistant response (different timestamp from user message)
        let assistant_message = Message {
            role: MessageRole::Assistant,
            content: model_response.output.clone(),
            timestamp: now_ms(),
            msg_type: None,
            summary_of: None,
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
}

/// Convert a stored Message to a ModelMessage for the LLM API call.
/// Compaction messages are transformed to System role with summary prefix.
fn message_to_model_message(message: &Message) -> ModelMessage {
    let (role, content) = match message.role {
        MessageRole::System => (ModelMessageRole::System, message.content.clone()),
        MessageRole::User => (ModelMessageRole::User, message.content.clone()),
        MessageRole::Assistant => (ModelMessageRole::Assistant, message.content.clone()),
        MessageRole::Compaction => (
            ModelMessageRole::System,
            format!("[Previous conversation summary]: {}", message.content),
        ),
    };

    ModelMessage { role, content }
}

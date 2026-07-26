pub mod error;
pub mod gateway;
pub mod models;
pub mod providers;
pub mod runtime;
pub mod skills;
pub mod compactor;
pub mod conversation_store;
pub mod engine;
pub mod topic_manager;
pub mod topic_store;
pub mod tool_registry;

pub use error::{AppError, AppResult};
pub use gateway::Gateway;
pub use models::{
    ChatModelSelection, ChatOptions, ChatResponse, CompactionConfig, Conversation,
    ConversationMode, Message, MessageRole, ModelCallRequest, ModelCallResponse, ModelCapabilities,
    ModelInfo, ModelMessage, ModelMessageRole, ProviderInfo, ProviderKind, RuntimeStatus,
    ScopeInItem, SkillInfo, ToolCall, ToolDefinition, Topic, TopicStatus, TopicUpdate,
};

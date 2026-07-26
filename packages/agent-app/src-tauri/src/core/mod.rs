pub mod error;
pub mod gateway;
pub mod models;
pub mod providers;
pub mod compactor;
pub mod conversation_store;
pub mod engine;
pub mod neuron_store;
pub mod topic_manager;
pub mod topic_store;
pub mod neuron_manager;
pub mod runtime_manager;
pub mod tool_registry;

pub use error::{AppError, AppResult};
pub use gateway::Gateway;
pub use models::{
    ChatModelSelection, ChatOptions, ChatResponse, CompactionConfig, Connection, Conversation,
    ConversationMode, Message, MessageRole, ModelCallRequest, ModelCallResponse, ModelCapabilities,
    ModelInfo, ModelMessage, ModelMessageRole, Neuron, NeuronUpdate, ProviderInfo, ProviderKind,
    RuntimeStatus, ScopeInItem, SkillInfo, ToolCall, ToolDefinition, Topic, TopicStatus,
    TopicUpdate,
};
pub use runtime_manager::{register_runtime_tools, RunningSession, RuntimeManager};

pub mod compactor;
pub mod conversation_store;
pub mod engine;
pub mod error;
pub mod gateway;
pub mod models;
pub mod neuron_config;
pub mod neuron_manager;
pub mod neuron_model;
pub mod neuron_store;
pub mod providers;
pub mod session_tracker;
pub mod tool_registry;
pub mod topic_manager;
pub mod topic_store;

pub use error::{AppError, AppResult};
pub use gateway::Gateway;
pub use models::{
    CandidateQuery, ChatModelSelection, ChatOptions, ChatResponse, CompactionConfig, Connection,
    Conversation, ConversationMode, GeneratedNeuronDraft, Message, MessageRole, ModelCallRequest,
    ModelCallResponse, ModelCapabilities, ModelInfo, ModelMessage, ModelMessageRole, Neuron,
    NeuronCreate, NeuronUpdate, ProviderInfo, ProviderKind, RuntimeStatus, ScopeInItem, SkillInfo,
    ToolCall, ToolDefinition, Topic, TopicStatus, TopicUpdate,
};
pub use session_tracker::{register_session_tracker_tools, RunningSession, SessionTracker};

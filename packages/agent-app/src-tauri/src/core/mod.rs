pub mod error;
pub mod gateway;
pub mod models;
pub mod providers;
pub mod runtime;
pub mod skills;
pub mod compactor;
pub mod conversation_store;
pub mod engine;

pub use error::{AppError, AppResult};
pub use gateway::Gateway;
pub use models::{
    ChatModelSelection, ChatOptions, ChatResponse, CompactionConfig, Conversation, Message,
    MessageRole, ModelCallRequest, ModelCallResponse, ModelCapabilities, ModelInfo, ModelMessage,
    ModelMessageRole, ProviderInfo, ProviderKind, RuntimeStatus, SkillInfo,
};

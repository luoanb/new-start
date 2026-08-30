pub mod agent_session;
pub mod app_log;
pub mod assistant_session;
pub mod chat_session;
pub mod cmd_exec;
pub mod compactor;
pub mod config;
pub mod current_time;
pub mod conversation_runner;
pub mod conversation_store;
pub mod context_safety;
pub mod dynamic_tool;
pub mod error;
pub mod events;
pub mod gateway;
pub mod hook;
/// 兼容别名：旧 `core::hook_judgement_store` 路径指向 `core/hook/store` 子模块，
/// 保持外部消费方引用不变（hook 收拢到 `core/hook/` 后零改动目标）。
pub mod hook_judgement_store {
    pub use super::hook::store::*;
}
pub mod insert_catalog;
pub mod log_phase;
pub mod log_redact;
pub mod mcp;
pub mod model_call_input;
pub mod models;
pub mod neuron;
/// 兼容别名：旧 `core::neuron_*` / `core::spec_manager` 路径指向 `core::neuron` 子模块，
/// 保持外部消费方引用不变（NeuronManager 拆分零改动目标）。
pub mod neuron_config {
    pub use super::neuron::config::*;
}
pub mod neuron_manager {
    pub use super::neuron::manager::*;
}
pub mod neuron_model {
    pub use super::neuron::model::*;
}
pub mod neuron_store {
    pub use super::neuron::store::*;
}
pub mod openai_compat;
pub mod poller;
pub mod poller_step;
pub mod providers;
pub mod round_executor;
pub mod round_resolver;
pub mod round_types;
pub mod session_coordinator;
pub mod session_tracker;
pub mod spec_manager {
    pub use super::neuron::spec::*;
}
pub mod storage;
pub mod tool_config;
pub mod tool_registry;
pub mod topic_manager;
pub mod topic_store;
pub use topic_store::TopicStore;
pub use hook_judgement_store::HookJudgementStore;
pub use hook::{
    hook_def, hook_defs_meta, AttemptRecord, HookDef, HookDefMeta, JudgementAnchor,
    JudgementOutcome, JudgementStatus,
};

pub use assistant_session::{
    AssistantSession, SYSTEM_TYPE_ROUND_REVIEW, SYSTEM_TYPE_SELECT_NEURON,
    SYSTEM_TYPE_USER_ROUND_JUDGEMENT,
};
pub use conversation_runner::ConversationRunner;
pub use error::{AppError, AppResult};
pub use events::{StateChange, StateEmitter, STATE_CHANGED_EVENT};
pub use gateway::Gateway;
pub use insert_catalog::InsertCatalog;
pub use mcp::{McpServerClient, McpServerStatus, McpServerStatusKind};
pub use model_call_input::{ModelAppendTemplate, ModelCallInput};
pub use round_types::{RoundOutcome, SessionSeed, SessionState};
pub use models::{
    AssistantCandidateScope, BootstrapReport, CandidateQuery, ChatModelSelection, ChatOptions,
    ChatResponse, CompactionConfig, Connection, Conversation, ConversationMode,
    ConversationSummaryPage, CreateNeuronInput, EnsureSystemOpts, GeneratedNeuronDraft, Message,
    MessageBody, MessagePage, MessageRole, ModelCallRequest, ModelCallResponse, ModelCapabilities,
    ModelInfo, ModelMessage, ModelMessageRole, NeighborhoodPoolPolicy, Neuron, NeuronCreate,
    NeuronKindFilter, NeuronPage, NeuronSubgraph, NeuronUpdate, ProviderInfo, ProviderKind,
    RuntimeStatus, SamplingParams, ScopeInItem, SelectionPolicy, SessionBehavior, SkillInfo,
    SystemPromptStatus, ThinkingCapability, ThinkingConfig, ThinkingEffort, ToolCall,
    ToolDefinition, ToolInfo, ToolPolicy, ToolSource, Topic, TopicStatus, TopicUpdate,
    DEFAULT_ASSISTANT_GLOBAL_LIMIT,
};
pub use poller::{
    PollHandler, Poller, PollerConfigReader, PollerRunState, PollerSettings, PollerStatus,
    DEFAULT_ASSISTANT_POLL_TICKS, DEFAULT_POLLER_BASE_INTERVAL_MS,
};
pub use session_tracker::{register_session_tracker_tools, RunningSession, SessionTracker};

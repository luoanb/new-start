//! 服务器公共运行时：Gateway 与分域服务的统一初始化。
//!
//! 供 Tauri GUI（`lib.rs::run`）与 headless 服务器（`bin/pulsar-server.rs`）复用，
//! 避免两套初始化逻辑行为分叉。调用方负责提供 `StateEmitter` 与 `TerminalEventHub`
//! （GUI 走 AppHandle 事件；headless 用空发射器 + `new_headless`）。

use std::{path::Path, sync::Arc};

use crate::core::{
    assistant_session::AssistantSession,
    conversation_store::ConversationStore,
    neuron_manager::NeuronManager,
    poller::Poller,
    providers::ProviderRegistry,
    session_tracker::SessionTracker,
    topic_store::TopicStore,
    Gateway, StateEmitter,
};
use crate::terminal::{AgentTerminalBridge, TerminalEventHub, TerminalManager};

/// 服务器运行上下文：Gateway + 分域服务的句柄 + 终端设施。
pub struct ServerRuntime {
    pub gateway: Gateway,
    pub neuron_manager: Arc<NeuronManager>,
    pub topic_store: Arc<std::sync::Mutex<TopicStore>>,
    pub assistant: Arc<AssistantSession>,
    pub poller: Arc<std::sync::Mutex<Poller>>,
    pub sessions: SessionTracker,
    pub providers: ProviderRegistry,
    pub conversation_store: ConversationStore,
    pub terminal_manager: Arc<TerminalManager>,
    pub terminal_bridge: Arc<AgentTerminalBridge>,
    pub terminal_hub: TerminalEventHub,
}

/// 组装 Gateway 与分域服务（核心初始化逻辑的唯一入口）。
pub fn build_server_runtime(
    storage_root: &Path,
    state_emit: StateEmitter,
    terminal_hub: TerminalEventHub,
) -> Result<ServerRuntime, String> {
    let store = ConversationStore::new(storage_root).map_err(|error| error.to_string())?;
    // 终端桥接（方案 A）：先于 Gateway 创建，注入 execute_command 可见执行能力；
    // 同一 manager 由 command 层（app.manage）与 Agent 工具桥接共用。
    let terminal_manager = Arc::new(TerminalManager::new());
    let terminal_bridge = Arc::new(AgentTerminalBridge::new(
        Arc::clone(&terminal_manager),
        terminal_hub.clone(),
    ));
    let gateway = Gateway::with_state_emitter_and_terminal(
        store,
        Some(state_emit),
        Some(terminal_bridge.clone()),
    )
    .map_err(|error| error.to_string())?;

    // Domain states (no outer Mutex across network).
    let neuron_manager = gateway.neuron_manager();
    let topic_store = gateway.topic_store().map_err(|e| e.to_string())?;
    let assistant = gateway.assistant();
    let poller = gateway.poller();
    let sessions = gateway.session_tracker();
    let providers = gateway.providers();
    let conversation_store = gateway.conversation_store();

    Ok(ServerRuntime {
        gateway,
        neuron_manager,
        topic_store,
        assistant,
        poller,
        sessions,
        providers,
        conversation_store,
        terminal_manager,
        terminal_bridge,
        terminal_hub,
    })
}

//! 封闭核心契约：轮次服务的稳定输入、输出与不变量（架构设计 §3）。
//!
//! 本模块只含契约类型与 trait，不含轮次实现；Entry Adapter、Driver 与 Hook 只能
//! 通过这些契约进入核心，不得直接调用 Store、ModelPort 或 ToolExecutor 拼自己的轮次。
//! 固定不变量（§2.2 / §3.7）：会话串行、输入先落库、工具配对、结果提交顺序、
//! 事件事实性、取消语义（已落库输入不回滚）。
//!
//! 执行产物（模型侧结果）在 [`super::round_types::RoundProduct`]；本模块的
//! [`RoundOutcome`] 是驱动层唯一需要理解的轮次结果，由核心在轮次结束时组装。

use async_trait::async_trait;

use super::error::AppResult;
use super::events::StateChange;
use super::models::{
    AuthorizedToolCall, ChatResponse, Conversation, ConversationMode, Message, ModelRequest,
    ModelResponse, ToolDefinition, ToolTag,
};
use super::round_executor::ModelCaller;
use super::round_service::{read_session_state, session_seed};
use super::round_types::{SessionSeed, SessionState, ToolResult};

/// 会话标识新类型：包装现有字符串 id（M1 约定）。
///
/// wire 与存储中的字符串表示不变，仅在核心边界内使用本类型。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConversationId(String);

impl ConversationId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for ConversationId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ConversationId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

/// 单轮输入记录：决定输入侧如何落库与初始 `model_input`。
#[derive(Debug, Clone)]
pub enum InputRecord {
    /// 用户输入：落 user 消息；model_input = 文本。
    User(String),
    /// 轮询推进：model_input 由 before hook 拼（简报）；nudge 落库由 hook 决定
    /// （nudge_persist，简报刷新时才落，「生成一次，落库一次」）。
    Nudge,
    /// 无输入落库，但携带推进文本（Agent 后续轮）。
    Continue(String),
    /// 无输入、无初始文本（默认推进，简报由 hook 注入）。
    None,
}

/// 发起轮次的业务模式（架构设计 §5.1 四驱动：Chat / Agent / Assistant / Poller）。
///
/// 随 [`RoundRequest`] 传递；核心据此取默认工具授权（Agent → 目录全量，其余 → 选中
/// 神经元的 `tool_ids`）；输入处理形态仍由 [`InputRecord`] 推导，两者的一致性由 Driver 保证。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundMode {
    Chat,
    Agent,
    Assistant,
    Poller,
}

/// 轮次终态：正常完成 / 被取消（用户抢占或停止）/ 会话忙跳过。
///
/// 执行错误不走状态（[`RoundService::run`] 返回 `Err`），保持领域错误语义
/// （架构设计 §3.7：取消后不再启动新的外部调用，返回 Cancelled）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundStatus {
    Completed,
    Cancelled,
    Skipped,
}

/// 驱动层唯一需要理解的轮次结果（架构设计 §3.6）。
///
/// `session_id` 是实际执行会话（IP-1 hook 可切换会话，可能与请求不同）；
/// `tool_calls_declared` 表示模型本轮是否声明了能力调用（Agent 驱动续轮判断依据）；
/// `status` 表达轮次终态（`Result::Err` 之外的全部出口都在此表达）。
#[derive(Debug, Clone)]
pub struct RoundOutcome {
    pub session_id: ConversationId,
    pub response: ChatResponse,
    pub tool_calls_declared: bool,
    pub status: RoundStatus,
}

/// 一轮的请求（架构设计 §3.1）：只表达「对哪个会话、以什么输入、按哪种业务模式」。
///
/// 模型选型归会话：核心 `load_context` 从会话运行态（`SessionState.model`）读取本轮模型，
/// 不再随请求传入。工具授权与思考配置默认值由核心按 [`RoundMode`] / 触发类型推导
/// （见 [`super::round_policy`]），请求不再携带覆盖字段。
#[derive(Debug, Clone)]
pub struct RoundRequest {
    pub session_id: ConversationId,
    pub input: InputRecord,
    pub mode: RoundMode,
}

/// 核心唯一入口（架构设计 §3.1）：`run` 执行一轮，`cancel` 请求取消活动轮次。
///
/// 会话级串行（同会话同时最多一个活动轮次）由核心持有：User 轮抢占、非 User 轮
/// 遇忙跳过（`RoundStatus::Skipped`）。
#[async_trait]
pub trait RoundService: Send + Sync {
    async fn run(&self, request: RoundRequest) -> AppResult<RoundOutcome>;

    /// 流式执行一轮：`on_delta` 每块增量回调；轮次终态与 `run` 一致。
    async fn run_stream(
        &self,
        request: RoundRequest,
        on_delta: Box<dyn FnMut(StreamDelta) + Send>,
    ) -> AppResult<RoundOutcome>;

    /// 请求取消该会话当前活动轮次；无活动轮次时为无害 no-op。
    /// 取消经核心到达（入口不得绕过核心直接终止轮次），已落库输入不回滚。
    async fn cancel(&self, session_id: &ConversationId) -> AppResult<()>;
}

/// 驱动器契约（架构设计 §5.1）：驱动器只拥有循环策略，不拥有轮次实现。
///
/// `first` 为首轮请求（对哪个会话、什么输入、哪种业务模式）；续轮请求由驱动按策略派生
/// （Agent：`InputRecord::Continue` 固定指令，不落库、仅进入本轮模型输入）。
/// 四驱动共享同一 `RoundService`，不建立继承层次。
#[async_trait]
pub trait RoundDriver: Send + Sync {
    async fn run(&self, first: RoundRequest) -> AppResult<RoundOutcome>;

    async fn run_stream(
        &self,
        first: RoundRequest,
        on_delta: Box<dyn FnMut(StreamDelta) + Send>,
    ) -> AppResult<RoundOutcome>;
}

// ---------------------------------------------------------------------------
// 扩展端口契约（架构设计 §3.3–§3.6）：扩展实现只能通过这些端口进入核心。
// ---------------------------------------------------------------------------

/// 统一模型请求/响应即领域契约类型（供应商协议差异由 Provider Adapter 抹平）。
///
/// 模型端口（架构设计 §3.3）：核心只依赖统一请求与响应。
///
/// Provider 可以选择协议、模型和重试方式，但必须将外部响应转换为统一结果；
/// 传输重试不得重复提交已经产生业务副作用的能力调用。
///
/// 现有 [`ModelCaller`] 的全部实现方（生产 `ProviderRegistry` 与测试替身）
/// 经下方 blanket 适配自动满足本端口。
#[async_trait]
pub trait ModelPort: Send + Sync {
    async fn complete(&self, request: ModelRequest) -> AppResult<ModelResponse>;
}

#[async_trait]
impl<T: ModelCaller + ?Sized> ModelPort for T {
    async fn complete(&self, request: ModelRequest) -> AppResult<ModelResponse> {
        self.call_model(request).await
    }
}

/// 能力执行端口（架构设计 §3.4）：核心对每个模型声明的 `AuthorizedToolCall` 至多调用一次
/// `execute`，并将返回值规范化为 `ToolResult`；不能静默丢弃失败。
#[async_trait]
pub trait CapabilityExecutor: Send + Sync {
    async fn execute(&self, call: AuthorizedToolCall) -> AppResult<ToolResult>;
}

/// 工具目录只读端口（架构设计 §5.4）：核心的授权决策与 wire 工具声明只依赖本端口，
/// 不感知注册表实现（由 `tools::tool_registry::ToolRegistry` 实现）。
pub trait ToolCatalog: Send + Sync {
    /// 带指定标签的工具 id（注册顺序）。
    fn tools_with_tag(&self, tag: ToolTag) -> Vec<String>;
    /// 全量工具定义（授权白名单 ∩ 注册表用）。
    fn list_definitions(&self) -> Vec<ToolDefinition>;
    /// 指定 id 的工具定义（本轮 wire 声明用）。
    fn definitions_for(&self, tool_ids: &[String]) -> Vec<ToolDefinition>;
}

/// 流式占位更新载荷：收敛占位消息的正文 / 思维链 / 工具声明
/// （当前唯一「就地更新」场景；不改变消息顺序）。
#[derive(Debug, Clone)]
pub struct MessageTextUpdate {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Vec<AuthorizedToolCall>>,
}

/// 会话快照（架构设计 §3.2 / §3.5）：端口 `load` 的返回形状。
/// 核心只需真相源的「标识 + 模式 + 选型种子 + 运行态 + 消息」，不感知存储扩展字段。
#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    pub id: ConversationId,
    pub mode: ConversationMode,
    pub seed: Option<SessionSeed>,
    pub state: SessionState,
    pub messages: Vec<Message>,
}

impl SessionSnapshot {
    /// 自落库会话投影（wire / JSON 布局不变；`extra` 中的选型种子与运行态在此解出，
    /// 复用核心既有解析：`round_service::{session_seed, read_session_state}`）。
    pub fn from_conversation(conversation: &Conversation) -> Self {
        Self {
            id: ConversationId::new(&conversation.id),
            mode: conversation.mode.clone(),
            seed: session_seed(conversation),
            state: read_session_state(conversation),
            messages: conversation.messages.clone(),
        }
    }
}

/// 落库投影（架构设计 §3.5）：`RoundOutcome` 的持久化形态。
/// 由核心在持久化阶段组装，Store 不接触驱动层语义。
#[derive(Debug, Clone)]
pub struct PersistedOutcome {
    pub session_id: ConversationId,
    pub status: RoundStatus,
    /// 本轮产物消息（按 wire 顺序；含模型声明、工具结果与最终回复）。
    pub messages: Vec<Message>,
}

/// 持久化端口（架构设计 §3.5）：会话真相源读写。
///
/// 端口分两组方法：
/// - **核心管线原语（同步）**：核心轮次管线实际使用的真相源读写（读会话、追加消息、
///   就地更新流式占位、写回元数据）。JSON 实现是本地文件 + 可重入锁、无 IO 等待，故为同步；
///   介质替换实现必须保持同样的顺序与一致性语义。
/// - **契约面（异步）**：§3.5 规定的读 / 追加 / 写回原语形态，供介质替换与契约测试使用。
///   其中 `append_input` / `append_outcome` 对应「输入先落库、产物后落库」的同名阶段（§3.5）。
///
/// 实现可替换介质（JSON / SQLite），但不得把追加改成无序覆盖，
/// 也不得在输入落库后悄悄删除输入（§2.2）。输入先落库、产物后落库的
/// 调用顺序由核心管线规定（§3.5），端口只提供追加原语。
#[async_trait]
pub trait ConversationStore: Send + Sync {
    // ── 核心管线原语（同步）──
    /// 读取会话真相源（含全部消息）；不存在时返回领域错误。
    fn require_conversation(&self, id: &str) -> AppResult<Conversation>;
    /// 按顺序追加单条消息（输入落库 / 产物落库共用原语），返回追加后的会话。
    fn add_message(&self, id: &str, message: Message) -> AppResult<Conversation>;
    /// 就地更新指定索引消息（流式占位收敛；不改变消息数量与顺序），返回更新后的会话。
    fn update_message_at(
        &self,
        id: &str,
        index: usize,
        patch: &mut dyn FnMut(&mut Message),
    ) -> AppResult<Conversation>;
    /// 写回会话（元数据 / 运行态）。
    fn save_conversation(&self, conversation: &Conversation) -> AppResult<()>;

    // ── 契约面（异步，§3.5）──
    /// 读取会话真相源快照（含全部消息）；不存在时返回领域错误。
    async fn load(&self, id: &ConversationId) -> AppResult<SessionSnapshot>;
    /// 按传入顺序追加输入消息（对应「输入先落库」阶段；输入先落库、产物后落库）。
    async fn append_input(&self, id: &ConversationId, items: &[Message]) -> AppResult<()>;
    /// 按传入顺序追加本轮产物消息（对应「产物后落库」阶段；输入先落库、产物后落库）。
    async fn append_outcome(&self, id: &ConversationId, outcome: &PersistedOutcome) -> AppResult<()>;
    /// 就地更新指定索引消息的文本载荷（流式占位收敛）。
    async fn update_text_at(
        &self,
        id: &ConversationId,
        index: usize,
        update: MessageTextUpdate,
    ) -> AppResult<()>;
    /// 写回会话（元数据 / 运行态）。
    async fn save(&self, conversation: &Conversation) -> AppResult<()>;
}

/// 活动轮次的流式增量载荷（架构设计 §2.4 / §3.6）：瞬态渲染用，不落库、不作业务判断；
/// `done: true` 表示本轮完成，消费方应收敛为全量重拉。
#[derive(Debug, Clone)]
pub struct StreamDelta {
    pub message_index: usize,
    /// 该消息当前累积正文全文。
    pub content: String,
    /// 该消息当前累积思考全文（空串 = 无思考）。
    pub reasoning: String,
    pub done: bool,
}

/// 领域事件（架构设计 §2.4 / §3.6）：
/// - `Fact`：已提交事实（会话受影响、课题变化、工具状态变化等）；
/// - `Delta`：活动轮次的瞬态流式输出片段——不是事实、不表达终态；消费方只可用于渲染，
///   不得持久化为会话内容或作为业务判断依据，轮次提交后以事实事件为准。
#[derive(Debug, Clone)]
pub enum DomainEvent {
    Fact(StateChange),
    Delta {
        conversation_id: String,
        delta: StreamDelta,
    },
}

/// 事件出口（架构设计 §3.6）：事件唯一出口；消费者不得通过事件回写核心状态。
pub trait EventSink: Send + Sync {
    fn publish(&self, event: DomainEvent);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::*;
    use crate::core::error::AppError;
    use crate::core::models::{ConversationMode, MessageBody, MessageRole};

    #[test]
    fn conversation_id_wraps_string_representation() {
        let id = ConversationId::new("conv-1");
        assert_eq!(id.as_str(), "conv-1");
        assert_eq!(id.to_string(), "conv-1");
        assert_eq!(
            ConversationId::from(String::from("a")),
            ConversationId::from("a")
        );
        assert_eq!(id.clone().into_string(), "conv-1");
    }

    // ── ModelPort（§3.3）：blanket 适配覆盖全部 ModelCaller 实现方 ──

    struct EchoModelPort;

    #[async_trait]
    impl ModelCaller for EchoModelPort {
        async fn call_model(&self, request: ModelRequest) -> AppResult<ModelResponse> {
            Ok(ModelResponse {
                provider_id: request.provider_id,
                model_id: request.model_id,
                output: format!("echo:{}", request.messages.len()),
                tool_calls: None,
                finish_reason: "stop".into(),
                reasoning: None,
            })
        }
    }

    #[tokio::test]
    async fn model_port_serves_caller_implementations_via_blanket_adapter() {
        let port: Arc<dyn ModelPort> = Arc::new(EchoModelPort);
        let response = port
            .complete(ModelRequest {
                provider_id: "test".into(),
                model_id: "test-model".into(),
                messages: vec![crate::core::models::ModelMessage {
                    role: crate::core::models::ModelMessageRole::User,
                    content: "hi".into(),
                    tool_calls: None,
                    tool_call_id: None,
                    reasoning_content: None,
                }],
                tools: None,
                params: None,
                thinking: None,
                response_format: None,
            })
            .await
            .unwrap();
        assert_eq!(response.output, "echo:1");
    }

    // ── ConversationStore（§3.5）：内存替身 + JSON 真实实现 ──

    /// 内存替身：验证端口契约（追加顺序 / 就地更新 / 写回）。
    struct InMemoryStore {
        conversations: Mutex<HashMap<String, Conversation>>,
    }

    impl InMemoryStore {
        fn with(conversation: Conversation) -> Self {
            let mut map = HashMap::new();
            map.insert(conversation.id.clone(), conversation);
            Self {
                conversations: Mutex::new(map),
            }
        }
    }

    #[async_trait]
    impl ConversationStore for InMemoryStore {
        fn require_conversation(&self, id: &str) -> AppResult<Conversation> {
            self.conversations
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or_else(|| AppError::InvalidInput(format!("conversation not found: {id}")))
        }

        fn add_message(&self, id: &str, message: Message) -> AppResult<Conversation> {
            let mut guard = self.conversations.lock().unwrap();
            let conversation = guard
                .get_mut(id)
                .ok_or_else(|| AppError::InvalidInput(format!("conversation not found: {id}")))?;
            conversation.messages.push(message);
            Ok(conversation.clone())
        }

        fn update_message_at(
            &self,
            id: &str,
            index: usize,
            patch: &mut dyn FnMut(&mut Message),
        ) -> AppResult<Conversation> {
            let mut guard = self.conversations.lock().unwrap();
            let conversation = guard
                .get_mut(id)
                .ok_or_else(|| AppError::InvalidInput(format!("conversation not found: {id}")))?;
            if let Some(message) = conversation.messages.get_mut(index) {
                patch(message);
            }
            Ok(conversation.clone())
        }

        fn save_conversation(&self, conversation: &Conversation) -> AppResult<()> {
            self.conversations
                .lock()
                .unwrap()
                .insert(conversation.id.clone(), conversation.clone());
            Ok(())
        }

        async fn load(&self, id: &ConversationId) -> AppResult<SessionSnapshot> {
            let guard = self.conversations.lock().unwrap();
            let conversation = guard
                .get(id.as_str())
                .ok_or_else(|| AppError::InvalidInput(format!("conversation not found: {id}")))?;
            Ok(SessionSnapshot {
                id: ConversationId::new(id.as_str()),
                mode: conversation.mode.clone(),
                seed: None,
                state: Default::default(),
                messages: conversation.messages.clone(),
            })
        }

        async fn append_input(&self, id: &ConversationId, items: &[Message]) -> AppResult<()> {
            let mut guard = self.conversations.lock().unwrap();
            let conversation = guard
                .get_mut(id.as_str())
                .ok_or_else(|| AppError::InvalidInput(format!("conversation not found: {id}")))?;
            conversation.messages.extend(items.iter().cloned());
            Ok(())
        }

        async fn append_outcome(
            &self,
            id: &ConversationId,
            outcome: &PersistedOutcome,
        ) -> AppResult<()> {
            let mut guard = self.conversations.lock().unwrap();
            let conversation = guard
                .get_mut(id.as_str())
                .ok_or_else(|| AppError::InvalidInput(format!("conversation not found: {id}")))?;
            conversation.messages.extend(outcome.messages.iter().cloned());
            Ok(())
        }

        async fn update_text_at(
            &self,
            id: &ConversationId,
            index: usize,
            update: MessageTextUpdate,
        ) -> AppResult<()> {
            let mut guard = self.conversations.lock().unwrap();
            let conversation = guard
                .get_mut(id.as_str())
                .ok_or_else(|| AppError::InvalidInput(format!("conversation not found: {id}")))?;
            if let Some(message) = conversation.messages.get_mut(index) {
                if let MessageBody::Text {
                    content,
                    reasoning,
                    tool_calls,
                } = &mut message.body
                {
                    *content = update.content;
                    *reasoning = update.reasoning;
                    *tool_calls = update.tool_calls;
                }
            }
            Ok(())
        }

        async fn save(&self, conversation: &Conversation) -> AppResult<()> {
            self.conversations
                .lock()
                .unwrap()
                .insert(conversation.id.clone(), conversation.clone());
            Ok(())
        }
    }

    fn text_message(content: &str) -> Message {
        Message {
            role: MessageRole::User,
            body: MessageBody::Text {
                content: content.to_string(),
                reasoning: None,
                tool_calls: None,
            },
            timestamp: 0,
            neuron_id: None,
        }
    }

    fn chat_conversation(id: &str) -> Conversation {
        Conversation {
            id: id.to_string(),
            mode: ConversationMode::Chat,
            messages: Vec::new(),
            created_at: 0,
            updated_at: 0,
            extra: None,
        }
    }

    #[tokio::test]
    async fn conversation_store_port_preserves_append_order_and_in_place_update() {
        // 替身实现：顺序追加不被重排；就地更新不改变消息数量与顺序。
        let store: Arc<dyn ConversationStore> =
            Arc::new(InMemoryStore::with(chat_conversation("mem-1")));
        let id = ConversationId::new("mem-1");
        store
            .append_input(&id, &[text_message("first"), text_message("second")])
            .await
            .unwrap();
        let snapshot = store.load(&id).await.unwrap();
        assert_eq!(snapshot.messages.len(), 2);
        assert_eq!(snapshot.messages[0].text(), "first");
        assert_eq!(snapshot.messages[1].text(), "second");
        store
            .update_text_at(
                &id,
                1,
                MessageTextUpdate {
                    content: "converged".into(),
                    reasoning: None,
                    tool_calls: None,
                },
            )
            .await
            .unwrap();
        let snapshot = store.load(&id).await.unwrap();
        assert_eq!(snapshot.messages.len(), 2, "就地更新不得增删消息");
        assert_eq!(snapshot.messages[1].text(), "converged");
    }

    #[tokio::test]
    async fn conversation_store_json_impl_satisfies_port() {
        let root = std::env::temp_dir().join(format!(
            "pulsar-port-store-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let concrete = crate::stores::conversation_store::JsonConversationStore::new(&root).unwrap();
        let id = ConversationId::new("port-conv");
        concrete
            .create_conversation(Some(id.as_str().to_string()), ConversationMode::Chat)
            .unwrap();
        let store: Arc<dyn ConversationStore> = Arc::new(concrete);

        store
            .append_input(&id, &[text_message("hello")])
            .await
            .unwrap();
        let snapshot = store.load(&id).await.unwrap();
        assert_eq!(snapshot.messages.len(), 1);
        store
            .update_text_at(
                &id,
                0,
                MessageTextUpdate {
                    content: "patched".into(),
                    reasoning: None,
                    tool_calls: None,
                },
            )
            .await
            .unwrap();
        let snapshot = store.load(&id).await.unwrap();
        assert_eq!(snapshot.messages[0].text(), "patched");
        let _ = std::fs::remove_dir_all(&root);
    }
}

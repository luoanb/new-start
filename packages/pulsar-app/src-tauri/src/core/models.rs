use serde::{Deserialize, Serialize};

use serde_json;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
    Compaction,
}

/// 消息内容体：作者（role）与内容类型（body）正交。
/// `kind` 为判别字段，前端按 `body.kind` 分支渲染。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageBody {
    /// 普通文本（user / assistant / system 正文）。
    Text { content: String },
    /// 工具调用（模型发起）：content 可为模型的说明文字。
    ToolCall {
        content: String,
        tool_calls: Vec<ToolCall>,
    },
    /// 工具返回：携带关联的调用 id 与工具名。
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        content: String,
    },
    /// 压缩摘要：summary_of 为被摘要消息的时间戳集合。
    Compaction {
        summary_of: Vec<String>,
        content: String,
    },
    /// 轮询推进简报：机器人自发推进时记录的模型输入（审计/展示用），不参与后续模型输入组装。
    Nudge { content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub role: MessageRole,
    pub body: MessageBody,
    pub timestamp: u128,
    /// 所属神经元（assistant 模式每轮选中，落库盖章；旧消息 / 非 assistant 模式为 None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neuron_id: Option<String>,
}

impl Message {
    /// 返回消息正文文本（各 body 变体共有的 content），用于展示 / 统计。
    pub fn text(&self) -> &str {
        match &self.body {
            MessageBody::Text { content }
            | MessageBody::ToolCall { content, .. }
            | MessageBody::ToolResult { content, .. }
            | MessageBody::Compaction { content, .. }
            | MessageBody::Nudge { content } => content,
        }
    }

    /// 是否为工具相关消息（调用或返回）。
    pub fn is_tool(&self) -> bool {
        matches!(
            self.body,
            MessageBody::ToolCall { .. } | MessageBody::ToolResult { .. }
        )
    }

    /// Compaction 摘要覆盖的消息时间戳集合。
    pub fn summary_of(&self) -> Option<&[String]> {
        match &self.body {
            MessageBody::Compaction { summary_of, .. } => Some(summary_of),
            _ => None,
        }
    }

    /// 工具调用消息的 tool_calls 数组。
    pub fn tool_calls(&self) -> Option<&[ToolCall]> {
        match &self.body {
            MessageBody::ToolCall { tool_calls, .. } => Some(tool_calls),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationMode {
    Chat,
    Agent,
    Assistant,
}

impl Default for ConversationMode {
    fn default() -> Self {
        Self::Chat
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Conversation {
    pub id: String,
    #[serde(default)]
    pub mode: ConversationMode,
    pub messages: Vec<Message>,
    pub created_at: u128,
    pub updated_at: u128,
    /// 会话级扩展（JSON 文件向后兼容旧字段）：规格会话写 `session` 键
    /// （`spec_neuron_id` + `state`），承载会话级运行态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

// ── Session specs (system neuron with behavior) ────────────────

/// 系统神经元的提示词取用策略（通用：`session.assistant_dialogue` 与裁决类系统神经元一视同仁，
/// 怎么取提示词都由 behavior.selection 决定；content = 业务语义，behavior = 程序可识别的业务入口）。
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub enum SelectionPolicy {
    /// 不取提示词：role_system 为空（不读任何 content）；insert_id 有则拼契约段。
    #[default]
    None,
    /// 固定：读系统神经元自己的 content，永不变化（不写 last_selected）。
    Fixed,
    /// 邻域：锚点 = last_selected_neuron_id（首轮 = 系统神经元自身），邻域池选 1。
    Neighborhood { policy: NeighborhoodPoolPolicy },
    /// 全域：无历史时全域池选 1 并写 last_selected；有历史时退化为 Neighborhood 邻域选。
    Global { limit: usize },
}

impl<'de> Deserialize<'de> for SelectionPolicy {
    /// 宽容解析：兼容旧 behavior JSON（`Fixed{neuron_id}` / `Global{switching}` /
    /// `Neighborhood{switching}` 的额外字段忽略或回落新语义）。
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Some(name) = value.as_str() {
            return match name {
                "None" => Ok(SelectionPolicy::None),
                "Fixed" => Ok(SelectionPolicy::Fixed),
                other => Err(serde::de::Error::custom(format!(
                    "unknown SelectionPolicy variant: {other}"
                ))),
            };
        }
        let obj = value.as_object().ok_or_else(|| {
            serde::de::Error::custom("SelectionPolicy must be a string or an object")
        })?;
        for (key, inner) in obj {
            match key.as_str() {
                "None" => return Ok(SelectionPolicy::None),
                // 旧 `Fixed{neuron_id}`：新语义读自己 content，忽略旧目标 id。
                "Fixed" => return Ok(SelectionPolicy::Fixed),
                "Global" => {
                    let limit = inner
                        .get("limit")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(DEFAULT_ASSISTANT_GLOBAL_LIMIT as u64)
                        as usize;
                    return Ok(SelectionPolicy::Global { limit });
                }
                "Neighborhood" => {
                    let policy = inner
                        .get("policy")
                        .and_then(|p| {
                            serde_json::from_value::<NeighborhoodPoolPolicy>(p.clone()).ok()
                        })
                        .unwrap_or_default();
                    return Ok(SelectionPolicy::Neighborhood { policy });
                }
                _ => {}
            }
        }
        Err(serde::de::Error::custom("unknown SelectionPolicy variant"))
    }
}

/// 会话规格的工具授权策略。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum ToolPolicy {
    /// 不授权任何工具。
    #[default]
    None,
    /// 本轮 role 神经元的 `tool_ids ∩ 注册表`。
    FromNeuron,
    /// 显式白名单 `∩ 注册表`。
    Allowlist(Vec<String>),
}

/// 会话规格（承载于 `system_type = 'session.<id>'` 的系统神经元的 behavior 列）。
/// 无 template / 短路开关字段：拼接规则由 `insert_id` 有无推导；
/// 单候选短路（候选池仅 1 个 → 跳过选型模型）为不可配置硬规则。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SessionBehavior {
    #[serde(default)]
    pub selection: SelectionPolicy,
    #[serde(default)]
    pub tools: ToolPolicy,
    /// 契约正文来源（InsertCatalog::require）；有值 → Manual 契约段，无值 → Neuron 角色拼接。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatResponse {
    pub conversation_id: String,
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatModelSelection {
    pub provider_id: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatOptions {
    pub provider_id: String,
    pub model_id: String,
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompactionConfig {
    pub enabled: bool,
    #[serde(default = "default_threshold_ratio")]
    pub threshold_ratio: f64,
    #[serde(default = "default_keep_last")]
    pub keep_last: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_ratio: 0.7,
            keep_last: 10,
        }
    }
}

fn default_threshold_ratio() -> f64 {
    0.7
}

fn default_keep_last() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeStatus {
    pub app_name: String,
    pub storage_path: String,
    pub current_conversation_id: String,
    pub skill_count: usize,
    pub conversation_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAi,
    OpenAiCompatible,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderInfo {
    pub id: String,
    pub display_name: String,
    pub api_base: Option<String>,
    pub auth_env: String,
    pub kind: ProviderKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCapabilities {
    pub chat: bool,
    pub tools: bool,
    pub streaming: bool,
    #[serde(default)]
    pub structured_output: bool,
    pub vision: Option<bool>,
    pub audio: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Map<String, serde_json::Value>>,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            chat: true,
            tools: false,
            streaming: false,
            structured_output: false,
            vision: None,
            audio: None,
            extras: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub provider_id: String,
    pub display_name: String,
    pub capabilities: ModelCapabilities,
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub pricing_input: Option<f64>,
    pub pricing_output: Option<f64>,
    pub pricing_cache_input: Option<f64>,
    pub knowledge_cutoff: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// 工具来源：native（项目自有代码工具）、config（配置驱动 DynamicTool）、mcp（外部 MCP server）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    #[default]
    Native,
    Config,
    Mcp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    /// 治理字段，不进模型请求 wire；缺省为 native（向后兼容）。
    #[serde(skip_serializing, default)]
    pub source: ToolSource,
}

/// 工具治理视图（供前端 DockPane 只读展示工具列表）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub source: ToolSource,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelCallRequest {
    pub provider_id: String,
    pub model_id: String,
    pub messages: Vec<ModelMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelCallResponse {
    pub provider_id: String,
    pub model_id: String,
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default)]
    pub finish_reason: String,
}

// ── Topic / Project Management ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeInItem {
    #[serde(default)]
    pub id: String,
    pub goal: String,
    pub done_contract: String,
    #[serde(default = "default_scope_in_status")]
    pub status: String,
}

fn default_scope_in_status() -> String {
    "pending".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TopicStatus {
    Todo,
    InProgress,
    Paused,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topic {
    pub id: String,
    pub name: String,
    pub status: TopicStatus,
    pub description: String,
    pub scope_in: Vec<ScopeInItem>,
    pub progress: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
    pub created_at: u128,
    pub updated_at: u128,
}

#[derive(Debug, Clone, Default)]
pub struct TopicUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub extra: Option<Option<serde_json::Value>>,
}

// ── Neuron / Knowledge Graph ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neuron {
    pub id: String,
    pub desc: String,
    pub content: String,
    pub weight: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_type: Option<String>,
    #[serde(default)]
    pub tool_ids: Vec<String>,
    pub created_at: u128,
    pub updated_at: u128,
    /// 命中/打分/编辑累积的活跃计数（回收排序用）。
    #[serde(default)]
    pub use_count: i64,
    /// 最近一次被使用的时间戳（命中/打分/编辑）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<u128>,
    /// 逻辑删除时间戳；非空表示已被回收，业务全流程不可见。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<u128>,
    /// 会话规格（仅 `system_type IS NOT NULL` 的神经元可挂载）；旧行/旧 JSON 缺失回落 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior: Option<SessionBehavior>,
}

/// 神经元分页结果（管理面列表：分页 + 搜索 + 类型筛选）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuronPage {
    pub items: Vec<Neuron>,
    pub total: usize,
    pub has_more: bool,
}

/// 管理面列表的类型筛选维度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeuronKindFilter {
    /// 全部（不区分系统/普通）。
    All,
    /// 仅系统神经元（`system_type IS NOT NULL`）。
    System,
    /// 仅普通神经元（`system_type IS NULL`）。
    Normal,
}

impl NeuronKindFilter {
    /// 从前端命令入参解析（"all" / "system" / "normal"），未知值回落 All。
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "system" => NeuronKindFilter::System,
            "normal" => NeuronKindFilter::Normal,
            _ => NeuronKindFilter::All,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NeuronCreate {
    pub desc: String,
    pub content: String,
    pub weight: f64,
    pub system_type: Option<String>,
    pub tool_ids: Vec<String>,
    /// Source neuron whose content generated this neuron (creator or variant id).
    pub lineage_parent_id: Option<String>,
    /// Variant pool state: `Some("active")` / `Some("observing")`; NULL means active.
    pub variant_state: Option<String>,
}

/// A variant neuron in a creator's candidate pool, with usage/score accumulators.
#[derive(Debug, Clone)]
pub struct NeuronVariant {
    pub neuron: Neuron,
    pub lineage_parent_id: Option<String>,
    pub use_count: i64,
    pub accumulated_delta: f64,
    pub last_used_at: Option<u128>,
    pub variant_state: Option<String>,
    pub manual_edited: bool,
}

/// Immutable version record of a neuron; `source` ∈ {seed, evolve, rollback}.
#[derive(Debug, Clone)]
pub struct NeuronVersion {
    pub id: String,
    pub neuron_id: String,
    pub content: String,
    pub source: String,
    pub created_at: u128,
    pub prev_version_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub source: String,
    pub target: String,
    pub weight: f64,
}

/// Ego-network subgraph returned by BFS around a seed neuron.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuronSubgraph {
    pub seed_id: String,
    pub neurons: Vec<Neuron>,
    pub connections: Vec<Connection>,
}

#[derive(Debug, Clone, Default)]
pub struct NeuronUpdate {
    pub desc: Option<String>,
    pub content: Option<String>,
    pub tool_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CandidateQuery {
    pub n: usize,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub min_new: usize,
}

pub const DEFAULT_ASSISTANT_GLOBAL_LIMIT: usize = 7;

/// Controllable quotas for an Assistant neighborhood candidate pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeighborhoodPoolPolicy {
    pub existing_downstream: usize,
    pub new_downstream: usize,
    pub fill_downstream_shortage: bool,
    pub siblings: usize,
    pub upstream_depth: usize,
    /// 全局权重 top N 补充配额：装配完邻域后并入全库 weight 最高的 N 个（按 id 去重）；0 = 不补充。
    pub global_top_weight: usize,
}

impl Default for NeighborhoodPoolPolicy {
    fn default() -> Self {
        Self {
            existing_downstream: 4,
            new_downstream: 2,
            fill_downstream_shortage: true,
            siblings: 2,
            upstream_depth: 3,
            global_top_weight: 5,
        }
    }
}

/// Scope and quotas for the first phase of Assistant neuron selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantCandidateScope {
    Global {
        limit: usize,
    },
    Neighborhood {
        self_id: String,
        policy: NeighborhoodPoolPolicy,
    },
}

impl AssistantCandidateScope {
    pub fn global_default() -> Self {
        Self::Global {
            limit: DEFAULT_ASSISTANT_GLOBAL_LIMIT,
        }
    }

    pub fn neighborhood_default(self_id: impl Into<String>) -> Self {
        Self::Neighborhood {
            self_id: self_id.into(),
            policy: NeighborhoodPoolPolicy::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CreateNeuronInput {
    Purpose(String),
    Messages(Vec<ModelMessage>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsureSystemOpts {
    pub reset: bool,
}

impl Default for EnsureSystemOpts {
    fn default() -> Self {
        Self { reset: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapReport {
    pub create_neuron_id: String,
    pub select_neuron_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPromptStatus {
    pub system_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neuron_id: Option<String>,
    /// 会话规格的 behavior 摘要（非 `session.%` 系统神经元为 None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior: Option<SessionBehavior>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedNeuronDraft {
    pub desc: String,
    pub content: String,
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub tool_ids: Vec<String>,
}

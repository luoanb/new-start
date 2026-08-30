//! `NeuronManager` 门面（Facade）：组合查询/选型/创建/演化 4 个领域服务，
//! 对外公开 API 与拆分前 `core::neuron_manager` 完全一致，方法体仅一行委托。
//!
//! 依赖方向：`manager → {query, selection, creation, evolution}`（无环）。

use std::sync::{Arc, Mutex, RwLock};

use crate::core::{
    error::AppResult,
    log_phase::PHASE_SELECT_ROLE,
    models::{
        AssistantCandidateScope, BootstrapReport, CandidateQuery, Connection, CreateNeuronInput,
        EnsureSystemOpts, ModelMessage, NeighborhoodPoolPolicy, Neuron, NeuronCreate,
        NeuronKindFilter, NeuronPage, NeuronSubgraph, NeuronUpdate, SelectionPolicy,
        SessionBehavior, SystemPromptStatus, ToolPolicy, DEFAULT_ASSISTANT_GLOBAL_LIMIT,
    },
    neuron::{
        config::NeuronConfigReader,
        creation::NeuronCreation,
        evolution::NeuronEvolution,
        model::NeuronModelCaller,
        query::NeuronQuery,
        selection::NeuronSelection,
        spec::SessionSpecManager,
        store::NeuronStore,
    },
    tool_registry::ToolRegistry,
};

use super::lock_error;

pub const CREATOR_SYSTEM_TYPE: &str = "create_neuron";
pub const ASSISTANT_SELECT_NEURON: &str = "assistant_select_neuron";
/// Spec alias for creator system_type.
pub const SYSTEM_CREATE: &str = CREATOR_SYSTEM_TYPE;
/// Spec alias for selector system_type.
pub const SYSTEM_SELECT: &str = ASSISTANT_SELECT_NEURON;
/// Known Assistant system prompts rebuilt by [`NeuronManager::rebootstrap`].
/// Does not include `create_neuron` (seed root).
/// 旧四条裁决（assistant_{match_topic,complete_scope,score_feedback,revise_topic}）已合并移除
/// （2026-08-30 spec）：存量神经元按惰性遗弃保留在库，rebootstrap 不再重建。
pub const REBOOTSTRAP_SYSTEM_TYPES: &[&str] = &[
    ASSISTANT_SELECT_NEURON,
    "assistant_user_round_judgement",
    "assistant_round_review",
];
pub(crate) const DEFAULT_SELECT_N: usize = DEFAULT_ASSISTANT_GLOBAL_LIMIT;
pub(crate) const MAX_CREATE_NEURON_COUNT: usize = 10;

/// 裁决类系统神经元 → 默认 behavior：`Fixed` + 各自契约段（统一入口兜底，行为与现状一致：
/// 用自己 content + 契约段）。非裁决类返回 `None`。
pub fn default_behavior_for_system_type(system_type: &str) -> Option<SessionBehavior> {
    let insert_id = match system_type {
        // 合并裁决（现行）。
        "assistant_user_round_judgement" => Some("assistant.user_round_judgement"),
        "assistant_round_review" => Some("assistant.round_review"),
        // 旧四条裁决（已合并）：保留兜底，存量遗留神经元缺 behavior 时仍可 backfill。
        "assistant_match_topic" => Some("assistant.match_topic"),
        "assistant_score_feedback" => Some("assistant.score_feedback"),
        "assistant_complete_scope" => Some("assistant.complete_scope"),
        "assistant_revise_topic" => Some("assistant.revise_topic"),
        _ => None,
    };
    insert_id.map(|id| SessionBehavior {
        selection: SelectionPolicy::Fixed,
        tools: ToolPolicy::None,
        insert_id: Some(id.to_string()),
    })
}

/// 门面：聚合 4 个领域服务，公开 API 与旧 `core::neuron_manager` 一致。
pub struct NeuronManager {
    /// 领域服务共享同一 store；Facade 本身仅测试/治理路径直接访问。
    #[allow(dead_code)]
    store: Arc<Mutex<NeuronStore>>,
    query: Arc<NeuronQuery>,
    selection: Arc<NeuronSelection>,
    creation: Arc<NeuronCreation>,
    evolution: Arc<NeuronEvolution>,
    /// 系统神经元 behavior 管理子组件（behavior 只读/写路径统一收敛于此；与创建域共享同一 store，
    /// 保留公开字段以兼容旧 API）。
    pub specs: SessionSpecManager,
}

impl std::fmt::Debug for NeuronManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeuronManager").finish_non_exhaustive()
    }
}

impl NeuronManager {
    pub fn new(
        store: Arc<Mutex<NeuronStore>>,
        model_caller: Arc<dyn NeuronModelCaller>,
        config: NeuronConfigReader,
        tool_registry: Arc<RwLock<ToolRegistry>>,
    ) -> Self {
        let query = Arc::new(NeuronQuery::new(Arc::clone(&store), config.clone()));
        let selection = Arc::new(NeuronSelection::new(
            Arc::clone(&store),
            Arc::clone(&model_caller),
            config.clone(),
            Arc::clone(&tool_registry),
        ));
        let creation = Arc::new(NeuronCreation::new(
            Arc::clone(&store),
            Arc::clone(&query),
            Arc::clone(&selection),
            config.clone(),
        ));
        let evolution = Arc::new(NeuronEvolution::new(
            Arc::clone(&store),
            Arc::clone(&model_caller),
            Arc::clone(&selection),
        ));
        Self {
            specs: SessionSpecManager::new(Arc::clone(&store)),
            store,
            query,
            selection,
            creation,
            evolution,
        }
    }

    /// 直接访问底层 store（测试与治理路径复用）。
    #[allow(dead_code)]
    fn store(&self) -> AppResult<std::sync::MutexGuard<'_, NeuronStore>> {
        self.store.lock().map_err(lock_error)
    }

    /// Previously registered AI tools; kept as no-op until tools are reintroduced with inserts.
    pub fn register_ai_tools(self: &Arc<Self>, _registry: &mut ToolRegistry) {}

    // ── 查询与治理（Query 域） ─────────────────────────────

    pub fn get(&self, id: &str) -> AppResult<Option<Neuron>> {
        self.query.get(id)
    }

    /// IPC-stable alias for [`Self::get`].
    pub fn get_neuron(&self, id: &str) -> AppResult<Option<Neuron>> {
        self.query.get_neuron(id)
    }

    pub fn get_by_system_type(&self, system_type: &str) -> AppResult<Option<Neuron>> {
        self.query.get_by_system_type(system_type)
    }

    /// IPC-stable alias for [`Self::get_by_system_type`].
    pub fn get_neuron_by_system_type(&self, system_type: &str) -> AppResult<Option<Neuron>> {
        self.query.get_neuron_by_system_type(system_type)
    }

    pub fn list(&self) -> AppResult<Vec<Neuron>> {
        self.query.list()
    }

    /// IPC-stable alias for [`Self::list`].
    pub fn list_neurons(&self) -> AppResult<Vec<Neuron>> {
        self.query.list_neurons()
    }

    pub fn connections(&self, id: &str) -> AppResult<Vec<Connection>> {
        self.query.connections(id)
    }

    /// IPC-stable alias for [`Self::connections`].
    pub fn get_connections(&self, id: &str) -> AppResult<Vec<Connection>> {
        self.query.get_connections(id)
    }

    pub fn network(&self, id: &str, max_depth: usize) -> AppResult<NeuronSubgraph> {
        self.query.network(id, max_depth)
    }

    /// IPC-stable alias for [`Self::network`].
    pub fn get_network(&self, id: &str, max_depth: usize) -> AppResult<NeuronSubgraph> {
        self.query.get_network(id, max_depth)
    }

    pub fn update_content_for_ai(&self, id: &str, update: NeuronUpdate) -> AppResult<Neuron> {
        self.query.update_content_for_ai(id, update)
    }

    pub fn update_content_for_admin(&self, id: &str, update: NeuronUpdate) -> AppResult<Neuron> {
        self.query.update_content_for_admin(id, update)
    }

    pub fn adjust_weight(&self, id: &str, delta: f64) -> AppResult<Neuron> {
        self.query.adjust_weight(id, delta)
    }

    pub fn adjust_edge_weight(
        &self,
        source: &str,
        target: &str,
        delta: f64,
    ) -> AppResult<Connection> {
        self.query.adjust_edge_weight(source, target, delta)
    }

    pub fn list_system_prompt_status(&self, types: &[&str]) -> AppResult<Vec<SystemPromptStatus>> {
        self.query.list_system_prompt_status(types)
    }

    /// Admin graph ops (not part of the unified creation front door).
    pub fn delete_for_admin(&self, id: &str) -> AppResult<bool> {
        let deleted = self.query.delete_for_admin(id)?;
        if deleted {
            // 跨域副作用：删除 creator 时失效选型域缓存（行为与拆分前一致）。
            self.selection.clear_creator_cache_if_matches(id);
        }
        Ok(deleted)
    }

    pub fn link_for_admin(&self, source: &str, target: &str, weight: f64) -> AppResult<Connection> {
        self.query.link_for_admin(source, target, weight)
    }

    pub fn unlink_for_admin(&self, source: &str, target: &str) -> AppResult<bool> {
        self.query.unlink_for_admin(source, target)
    }

    pub fn set_tool_ids_for_admin(&self, id: &str, tool_ids: Vec<String>) -> AppResult<Neuron> {
        self.query.set_tool_ids_for_admin(id, tool_ids)
    }

    /// 管理面分页列表（分页 + 搜索 + 类型筛选），供列表视图增量加载。
    pub fn list_neurons_page(
        &self,
        page: usize,
        page_size: usize,
        search: Option<&str>,
        kind: NeuronKindFilter,
    ) -> AppResult<NeuronPage> {
        self.query.list_neurons_page(page, page_size, search, kind)
    }

    /// 管理面设置 / 换绑 / 取消系统类型（system_type 唯一约束由 store 保证）。
    pub fn set_system_type_for_admin(
        &self,
        id: &str,
        system_type: Option<&str>,
    ) -> AppResult<Neuron> {
        self.query.set_system_type_for_admin(id, system_type)
    }

    /// 记录神经元使用信号（n=1 硬规则短路选中时调用）；忽略失败，不阻塞选择流程。
    pub fn mark_used_for_assistant(&self, neuron_id: &str) {
        self.query.mark_used_for_assistant(neuron_id);
    }

    /// 容量回收（超出 `neuron.capacity` 时回收低价值普通神经元，系统神经元豁免）。
    pub fn recycle_if_over_capacity(&self) -> AppResult<usize> {
        self.query.recycle_if_over_capacity()
    }

    // ── 候选池与选型（Selection 域） ───────────────────────

    pub async fn select_candidates(&self, query: CandidateQuery) -> AppResult<Vec<Neuron>> {
        self.selection.select_candidates(query).await
    }

    pub async fn select_one(&self, query: CandidateQuery) -> AppResult<Neuron> {
        self.selection.select_one(query).await
    }

    /// Select one neuron; `history` is read-only conversation context (not persisted by this call).
    pub async fn select_one_with_history(
        &self,
        query: CandidateQuery,
        history: &[ModelMessage],
    ) -> AppResult<Neuron> {
        self.selection.select_one_with_history(query, history).await
    }

    /// Build Assistant candidates without invoking the selection model.
    pub async fn select_assistant_candidates(
        &self,
        scope: AssistantCandidateScope,
    ) -> AppResult<Vec<Neuron>> {
        self.selection.select_assistant_candidates(scope).await
    }

    pub async fn select_one_from(&self, candidates: &[Neuron]) -> AppResult<Neuron> {
        self.selection.select_one_from(candidates).await
    }

    pub async fn select_one_from_with_history(
        &self,
        candidates: &[Neuron],
        history: &[ModelMessage],
        link_source: Option<&str>,
    ) -> AppResult<Neuron> {
        self.selection
            .select_one_from_with_history(candidates, history, link_source)
            .await
    }

    /// 惰性加载（或种子创建）creator 系统神经元；id 缓存命中直接返回。
    pub fn ensure_creator(&self) -> AppResult<Neuron> {
        self.selection.ensure_creator()
    }

    // ── 创建与启动编排（Creation 域） ──────────────────────

    /// Ordinary neuron(s) via unified creation flow (pool→7→1 under creator).
    /// `count` must be in `1..=10`. Model returns a JSON list of drafts; all are persisted.
    pub async fn create_neuron(
        &self,
        input: CreateNeuronInput,
        link_to: Option<&str>,
        count: usize,
    ) -> AppResult<Vec<Neuron>> {
        self.creation.create_neuron(input, link_to, count).await
    }

    /// Ensure a system prompt root (any system_type). Idempotent unless `opts.reset`.
    pub async fn ensure_system_neuron(
        &self,
        system_type: &str,
        opts: EnsureSystemOpts,
    ) -> AppResult<Neuron> {
        self.creation.ensure_system_neuron(system_type, opts).await
    }

    /// 懒创建系统神经元（复用 ensure_system_neuron 骨架：存在复用 / reset 重建）。
    pub async fn ensure_session_neuron(
        &self,
        system_type: &str,
        behavior: SessionBehavior,
        content: Option<String>,
        opts: EnsureSystemOpts,
    ) -> AppResult<Neuron> {
        self.creation
            .ensure_session_neuron(system_type, behavior, content, opts)
            .await
    }

    /// 只读：取系统神经元的 behavior（校验 system_type + behavior 非空）。
    pub fn get_session_behavior(&self, neuron_id: &str) -> AppResult<SessionBehavior> {
        self.creation.get_session_behavior(neuron_id)
    }

    /// 管理面更新入口：只写 behavior（specs 子组件），不触碰 content。
    pub fn update_behavior_for_admin(
        &self,
        id: &str,
        behavior: SessionBehavior,
    ) -> AppResult<Neuron> {
        self.creation.update_behavior_for_admin(id, behavior)
    }

    /// 列出所有 `session.%` 系统神经元（含 behavior 摘要）。
    pub fn list_session_specs(&self) -> AppResult<Vec<SystemPromptStatus>> {
        self.creation.list_session_specs()
    }

    /// Startup readiness: creator + selector only.
    pub async fn bootstrap(&self) -> AppResult<BootstrapReport> {
        self.creation.bootstrap().await
    }

    /// Force rebuild of known assistant system prompts (creator seed kept).
    pub async fn rebootstrap(&self) -> AppResult<BootstrapReport> {
        self.creation.rebootstrap().await
    }

    /// 直接持久化一条普通神经元（可选链接父节点），不走模型。
    pub fn create_plain(&self, create: NeuronCreate, link_to: Option<&str>) -> AppResult<Neuron> {
        self.creation.create_plain(create, link_to)
    }

    // ── 知识演化（Evolution 域） ───────────────────────────

    /// Bump `use_count` / `last_used_at` for a variant that was just used to
    /// generate a child neuron.
    pub fn record_variant_usage(&self, variant_id: &str) -> AppResult<Neuron> {
        self.evolution.record_variant_usage(variant_id)
    }

    /// Accumulate a signed score delta onto a variant (lineage attribution).
    pub fn accumulate_variant_delta(&self, variant_id: &str, delta: f64) -> AppResult<Neuron> {
        self.evolution.accumulate_variant_delta(variant_id, delta)
    }

    /// 变体状态机：观察→晋升 / 淘汰回滚 / 差分重写（每次调用只处理一次）。
    pub async fn maybe_evolve_creator_variants(&self) -> AppResult<()> {
        self.evolution.maybe_evolve_creator_variants().await
    }

    // ── 选型装配（converse 共用） ───────────────────────────────

    /// selection → 候选池装配 scope（`resolve_role` 委托）。
    /// `None` / `Fixed` 不涉及候选池，返回 `None`。
    ///
    /// - `Neighborhood`：有历史锚点 = last_selected；首轮锚点 = 发起神经元自身；
    /// - `Global`：无历史全域池选 1；有历史退化为邻域选（锚点 = last_selected）。
    pub(crate) fn scope_for_selection(
        selection: &SelectionPolicy,
        spec_neuron_id: &str,
        last_selected: Option<&str>,
    ) -> Option<AssistantCandidateScope> {
        match selection {
            SelectionPolicy::None | SelectionPolicy::Fixed => None,
            SelectionPolicy::Neighborhood { policy } => Some(match last_selected {
                Some(last) => AssistantCandidateScope::Neighborhood {
                    self_id: last.to_string(),
                    policy: *policy,
                },
                None => AssistantCandidateScope::Neighborhood {
                    self_id: spec_neuron_id.to_string(),
                    policy: *policy,
                },
            }),
            SelectionPolicy::Global { limit } => Some(match last_selected {
                Some(last) => AssistantCandidateScope::Neighborhood {
                    self_id: last.to_string(),
                    policy: NeighborhoodPoolPolicy::default(),
                },
                None => AssistantCandidateScope::Global { limit: *limit },
            }),
        }
    }

    /// role 解析/选型：按 scope 装配候选池；n=1 硬规则短路（跳过选型模型并记录使用信号）。
    pub(crate) async fn select_role(
        &self,
        messages: &[ModelMessage],
        scope: AssistantCandidateScope,
    ) -> AppResult<Neuron> {
        tracing::info!(
            phase = PHASE_SELECT_ROLE,
            scope = ?scope,
            history_len = messages.len(),
            "select_role entry"
        );
        // 回挂边锚点 = 候选池锚点：Neighborhood 的 self_id（非首轮 = last_selected / 首轮 = 发起神经元）；Global 无锚点 → None。
        let link_source = match &scope {
            AssistantCandidateScope::Neighborhood { self_id, .. } => Some(self_id.clone()),
            AssistantCandidateScope::Global { .. } => None,
        };
        let candidates = self.select_assistant_candidates(scope).await?;
        if candidates.len() == 1 {
            let single = candidates[0].clone();
            self.mark_used_for_assistant(&single.id);
            return Ok(single);
        }
        self.select_one_from_with_history(&candidates, messages, link_source.as_deref())
            .await
    }
}

#[cfg(test)]
mod tests;

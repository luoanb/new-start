use std::{
    collections::HashSet,
    sync::{Arc, Mutex, RwLock},
};

use async_trait::async_trait;
use serde_json::Value;

use super::{
    error::{AppError, AppResult},
    insert_catalog::InsertCatalog,
    model_call_input::{ModelAppendTemplate, ModelCallInput},
    models::{
        AssistantCandidateScope, BootstrapReport, CandidateQuery, Connection, CreateNeuronInput,
        EnsureSystemOpts, GeneratedNeuronDraft, ModelMessage, NeighborhoodPoolPolicy, Neuron,
        NeuronCreate, NeuronSubgraph, NeuronUpdate, NeuronVariant, SystemPromptStatus,
        DEFAULT_ASSISTANT_GLOBAL_LIMIT,
    },
    neuron_config::NeuronConfigReader,
    neuron_model::NeuronModelCaller,
    neuron_store::NeuronStore,
    tool_registry::{Tool, ToolRegistry},
};

pub const CREATOR_SYSTEM_TYPE: &str = "create_neuron";
pub const ASSISTANT_SELECT_NEURON: &str = "assistant_select_neuron";
/// Spec alias for creator system_type.
pub const SYSTEM_CREATE: &str = CREATOR_SYSTEM_TYPE;
/// Spec alias for selector system_type.
pub const SYSTEM_SELECT: &str = ASSISTANT_SELECT_NEURON;
/// Known Assistant system prompts rebuilt by [`NeuronManager::rebootstrap`].
/// Does not include `create_neuron` (seed root).
pub const REBOOTSTRAP_SYSTEM_TYPES: &[&str] = &[
    ASSISTANT_SELECT_NEURON,
    "assistant_match_topic",
    "assistant_complete_scope",
    "assistant_score_feedback",
];
const DEFAULT_SELECT_N: usize = DEFAULT_ASSISTANT_GLOBAL_LIMIT;
const MAX_CREATE_NEURON_COUNT: usize = 10;

pub struct NeuronManager {
    store: Arc<Mutex<NeuronStore>>,
    model_caller: Arc<dyn NeuronModelCaller>,
    config: NeuronConfigReader,
    creator_id: Mutex<Option<String>>,
    /// 共享工具注册表（与 Gateway 同一 `Arc<RwLock>`）：读锁 clone 后立即释放，不跨 await。
    tool_registry: Arc<RwLock<ToolRegistry>>,
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
        Self {
            store,
            model_caller,
            config,
            creator_id: Mutex::new(None),
            tool_registry,
        }
    }

    /// Build an "available tools" block for creator prompts so `tool_ids` can only
    /// be picked from tools that actually exist in the registry (no invented names).
    fn available_tools_block(&self) -> String {
        let defs = self
            .tool_registry
            .read()
            .map(|reg| reg.list_definitions())
            .unwrap_or_default();
        if defs.is_empty() {
            return "No tools are registered; `tool_ids` must be [].".to_string();
        }
        let mut lines = vec![
            "Available tools for `tool_ids` (pick only from this list; do not invent names):"
                .to_string(),
        ];
        for d in defs {
            lines.push(format!("- {}: {}", d.name, d.description));
        }
        lines.join("\n")
    }

    /// Previously registered AI tools; kept as no-op until tools are reintroduced with inserts.
    pub fn register_ai_tools(self: &Arc<Self>, _registry: &mut ToolRegistry) {}

    pub fn get(&self, id: &str) -> AppResult<Option<Neuron>> {
        self.store()?.get_neuron(id)
    }

    /// IPC-stable alias for [`Self::get`].
    pub fn get_neuron(&self, id: &str) -> AppResult<Option<Neuron>> {
        self.get(id)
    }

    pub fn get_by_system_type(&self, system_type: &str) -> AppResult<Option<Neuron>> {
        self.store()?.get_neuron_by_system_type(system_type)
    }

    /// IPC-stable alias for [`Self::get_by_system_type`].
    pub fn get_neuron_by_system_type(&self, system_type: &str) -> AppResult<Option<Neuron>> {
        self.get_by_system_type(system_type)
    }

    pub fn list(&self) -> AppResult<Vec<Neuron>> {
        self.store()?.list_neurons()
    }

    /// IPC-stable alias for [`Self::list`].
    pub fn list_neurons(&self) -> AppResult<Vec<Neuron>> {
        self.list()
    }

    pub fn connections(&self, id: &str) -> AppResult<Vec<Connection>> {
        self.store()?.get_connections(id)
    }

    /// IPC-stable alias for [`Self::connections`].
    pub fn get_connections(&self, id: &str) -> AppResult<Vec<Connection>> {
        self.connections(id)
    }

    pub fn network(&self, id: &str, max_depth: usize) -> AppResult<NeuronSubgraph> {
        self.store()?.get_network(id, max_depth)
    }

    /// IPC-stable alias for [`Self::network`].
    pub fn get_network(&self, id: &str, max_depth: usize) -> AppResult<NeuronSubgraph> {
        self.network(id, max_depth)
    }

    pub fn update_content_for_ai(&self, id: &str, update: NeuronUpdate) -> AppResult<Neuron> {
        let store = self.store()?;
        let neuron = store
            .get_neuron(id)?
            .ok_or_else(|| AppError::NeuronNotFound(id.to_string()))?;
        if neuron.system_type.is_some() {
            return Err(AppError::InvalidInput(
                "AI tools cannot update system neurons".into(),
            ));
        }
        store.update_neuron(id, update)
    }

    pub fn update_content_for_admin(&self, id: &str, update: NeuronUpdate) -> AppResult<Neuron> {
        let store = self.store()?;
        let updated = store.update_neuron(id, update)?;
        // Manual edits lock the neuron out of auto-rewrite / elimination.
        store.set_manual_edited(id, true)?;
        Ok(updated)
    }

    pub fn adjust_weight(&self, id: &str, delta: f64) -> AppResult<Neuron> {
        self.store()?.adjust_weight(id, delta)
    }

    pub fn adjust_edge_weight(
        &self,
        source: &str,
        target: &str,
        delta: f64,
    ) -> AppResult<Connection> {
        self.store()?
            .adjust_connection_weight(source, target, delta)
    }

    pub fn list_system_prompt_status(&self, types: &[&str]) -> AppResult<Vec<SystemPromptStatus>> {
        let mut out = Vec::with_capacity(types.len());
        for system_type in types {
            let neuron_id = self
                .get_by_system_type(system_type)?
                .map(|neuron| neuron.id);
            out.push(SystemPromptStatus {
                system_type: (*system_type).to_string(),
                neuron_id,
            });
        }
        Ok(out)
    }

    /// Admin graph ops (not part of the unified creation front door).
    pub fn delete_for_admin(&self, id: &str) -> AppResult<bool> {
        let deleted = self.store()?.delete_neuron(id)?;
        if deleted {
            let mut creator_id = self.creator_id.lock().map_err(lock_error)?;
            if creator_id.as_deref() == Some(id) {
                *creator_id = None;
            }
        }
        Ok(deleted)
    }

    pub fn link_for_admin(&self, source: &str, target: &str, weight: f64) -> AppResult<Connection> {
        self.store()?.link(source, target, weight)
    }

    pub fn unlink_for_admin(&self, source: &str, target: &str) -> AppResult<bool> {
        self.store()?.unlink(source, target)
    }

    pub fn set_tool_ids_for_admin(&self, id: &str, tool_ids: Vec<String>) -> AppResult<Neuron> {
        self.store()?.set_tool_ids(id, tool_ids)
    }

    pub async fn select_candidates(&self, query: CandidateQuery) -> AppResult<Vec<Neuron>> {
        if query.min_new > query.n {
            return Err(AppError::InvalidInput(
                "min_new must be less than or equal to n".into(),
            ));
        }
        if query.n == 0 {
            return Ok(Vec::new());
        }

        tracing::info!(
            phase = "select_candidates",
            n = query.n,
            min_new = query.min_new,
            source_id = query.source_id.as_deref().unwrap_or(""),
            "select_candidates start"
        );

        let source_id = self.resolve_source_id(query.source_id.as_deref())?;
        let mut selected = Vec::with_capacity(query.n);
        let mut selected_ids = HashSet::new();
        let mut created = 0usize;

        if query.min_new > 0 {
            let filled = self
                .fill_candidates_batch(source_id.as_deref(), query.min_new)
                .await?;
            created += filled.len();
            for neuron in filled {
                selected_ids.insert(neuron.id.clone());
                selected.push(neuron);
            }
        }

        let remaining = query.n - selected.len();
        let mut reused = 0usize;
        if remaining > 0 {
            let existing = {
                let store = self.store()?;
                match source_id.as_deref() {
                    Some(source_id) => {
                        store.list_direct_downstream(source_id, remaining, &selected_ids)?
                    }
                    None => store.list_global_candidates(remaining, &selected_ids)?,
                }
            };
            reused = existing.len();
            for neuron in existing {
                selected_ids.insert(neuron.id.clone());
                selected.push(neuron);
            }
        }

        let shortage = query.n.saturating_sub(selected.len());
        if shortage > 0 {
            let filled = self
                .fill_candidates_batch(source_id.as_deref(), shortage)
                .await?;
            created += filled.len();
            for neuron in filled {
                if selected_ids.insert(neuron.id.clone()) {
                    selected.push(neuron);
                }
            }
        }

        if selected.len() < query.n {
            return Err(AppError::NeuronBootstrapFailed(format!(
                "select_candidates could not fill pool: need {}, got {}",
                query.n,
                selected.len()
            )));
        }
        selected.truncate(query.n);

        tracing::info!(
            phase = "select_candidates",
            reused,
            created,
            total = selected.len(),
            candidate_ids = ?selected.iter().map(|n| n.id.clone()).collect::<Vec<_>>(),
            "select_candidates ok"
        );
        for (i, n) in selected.iter().enumerate() {
            tracing::info!(
                phase = "select_candidates.detail",
                index = i,
                id = %n.id,
                desc = %n.desc,
                weight = n.weight,
                tool_ids = ?n.tool_ids,
                content_len = n.content.len(),
                "select_candidates candidate entry"
            );
        }
        Ok(selected)
    }

    pub async fn select_one(&self, query: CandidateQuery) -> AppResult<Neuron> {
        self.select_one_with_history(query, &[]).await
    }

    /// Select one neuron; `history` is read-only conversation context (not persisted by this call).
    pub async fn select_one_with_history(
        &self,
        query: CandidateQuery,
        history: &[ModelMessage],
    ) -> AppResult<Neuron> {
        let mut query = query;
        if query.n == 0 {
            query.n = DEFAULT_SELECT_N;
        }
        let candidates = self.select_candidates(query).await?;
        self.select_one_from_with_history(&candidates, history)
            .await
    }

    /// Build Assistant candidates without invoking the selection model.
    pub async fn select_assistant_candidates(
        &self,
        scope: AssistantCandidateScope,
    ) -> AppResult<Vec<Neuron>> {
        match scope {
            AssistantCandidateScope::Global { limit } => {
                if limit == 0 {
                    return Err(AppError::InvalidInput(
                        "assistant global candidate limit must be >= 1".into(),
                    ));
                }
                self.select_candidates(CandidateQuery {
                    n: limit,
                    source_id: None,
                    min_new: 0,
                })
                .await
            }
            AssistantCandidateScope::Neighborhood { self_id, policy } => {
                self.select_neighborhood_candidates(&self_id, policy).await
            }
        }
    }

    async fn select_neighborhood_candidates(
        &self,
        self_id: &str,
        policy: NeighborhoodPoolPolicy,
    ) -> AppResult<Vec<Neuron>> {
        let _maximum_pool_size = policy
            .existing_downstream
            .checked_add(policy.new_downstream)
            .and_then(|total| total.checked_add(1))
            .and_then(|total| total.checked_add(policy.siblings))
            .and_then(|total| total.checked_add(policy.upstream_depth))
            .and_then(|total| total.checked_add(policy.global_top_weight))
            .ok_or_else(|| {
                AppError::InvalidInput("assistant candidate quotas overflow usize".into())
            })?;
        let self_neuron = self
            .get_neuron(self_id)?
            .ok_or_else(|| AppError::NeuronNotFound(self_id.to_string()))?;

        let mut selected = Vec::new();
        let mut selected_ids = HashSet::new();
        let child_exclusions = HashSet::from([self_id.to_string()]);
        let existing_children = self.store()?.list_direct_downstream(
            self_id,
            policy.existing_downstream,
            &child_exclusions,
        )?;
        let child_shortage = if policy.fill_downstream_shortage {
            policy
                .existing_downstream
                .saturating_sub(existing_children.len())
        } else {
            0
        };
        let new_child_count = policy
            .new_downstream
            .checked_add(child_shortage)
            .ok_or_else(|| {
                AppError::InvalidInput("assistant new downstream quota overflows usize".into())
            })?;
        if new_child_count > MAX_CREATE_NEURON_COUNT {
            return Err(AppError::InvalidInput(format!(
                "assistant new downstream count must be <={MAX_CREATE_NEURON_COUNT}, got {new_child_count}"
            )));
        }
        let new_children = self
            .fill_candidates_batch(Some(self_id), new_child_count)
            .await?;

        for neuron in existing_children.into_iter().chain(new_children) {
            if selected_ids.insert(neuron.id.clone()) {
                selected.push(neuron);
            }
        }
        if selected_ids.insert(self_neuron.id.clone()) {
            selected.push(self_neuron);
        }

        let direct_parent = self.store()?.select_direct_upstream(self_id)?;
        if let Some(parent) = direct_parent {
            let siblings = self.store()?.list_direct_downstream(
                &parent.id,
                policy.siblings,
                &selected_ids,
            )?;
            for sibling in siblings {
                if selected_ids.insert(sibling.id.clone()) {
                    selected.push(sibling);
                }
            }

            let mut ancestor = Some(parent);
            for _ in 0..policy.upstream_depth {
                let Some(current) = ancestor else {
                    break;
                };
                if selected_ids.insert(current.id.clone()) {
                    selected.push(current.clone());
                }
                ancestor = self.store()?.select_direct_upstream(&current.id)?;
            }
        }

        // 全局权重 top N 补充：保证高分节点在任意轮次都有机会被 LLM 选中（按 id 去重）。
        if policy.global_top_weight > 0 {
            let top = self
                .store()?
                .list_global_candidates(policy.global_top_weight, &selected_ids)?;
            for neuron in top {
                if selected_ids.insert(neuron.id.clone()) {
                    selected.push(neuron);
                }
            }
        }

        tracing::info!(
            phase = "select_assistant_candidates",
            self_id,
            existing_downstream = policy.existing_downstream,
            new_downstream = policy.new_downstream,
            fill_downstream_shortage = policy.fill_downstream_shortage,
            siblings = policy.siblings,
            upstream_depth = policy.upstream_depth,
            global_top_weight = policy.global_top_weight,
            total = selected.len(),
            candidate_ids = ?selected.iter().map(|n| n.id.clone()).collect::<Vec<_>>(),
            "assistant neighborhood candidates assembled"
        );
        Ok(selected)
    }

    pub async fn select_one_from(&self, candidates: &[Neuron]) -> AppResult<Neuron> {
        self.select_one_from_with_history(candidates, &[]).await
    }

    pub async fn select_one_from_with_history(
        &self,
        candidates: &[Neuron],
        history: &[ModelMessage],
    ) -> AppResult<Neuron> {
        if candidates.is_empty() {
            return Err(AppError::InvalidInput(
                "No neuron candidates available for selection".into(),
            ));
        }
        match self.try_llm_select(candidates, history).await {
            Ok(neuron) => {
                // 活跃信号：select_one 命中即记录使用（忽略失败，不阻塞选择流程）。
                let _ = self.store()?.mark_used(&neuron.id);
                tracing::info!(
                    phase = "select_one",
                    method = "llm",
                    neuron_id = %neuron.id,
                    "select_one ok"
                );
                Ok(neuron)
            }
            Err(error) => {
                tracing::warn!(
                    phase = "select_one",
                    method = "weight_fallback",
                    error = %error,
                    "llm select failed; falling back to weight"
                );
                let picked = pick_by_weight(candidates)?;
                let _ = self.store()?.mark_used(&picked.id);
                Ok(picked)
            }
        }
    }

    /// Ordinary neuron(s) via unified creation flow (pool→7→1 under creator).
    /// `count` must be in `1..=10`. Model returns a JSON list of drafts; all are persisted.
    pub async fn create_neuron(
        &self,
        input: CreateNeuronInput,
        link_to: Option<&str>,
        count: usize,
    ) -> AppResult<Vec<Neuron>> {
        if count == 0 || count > MAX_CREATE_NEURON_COUNT {
            return Err(AppError::InvalidInput(format!(
                "create_neuron count must be 1..={MAX_CREATE_NEURON_COUNT}, got {count}"
            )));
        }

        let creator = self.ensure_creator()?;
        let filling_creator = link_to == Some(creator.id.as_str());
        let (prompt_content, lineage_parent_id) = if filling_creator {
            // Seed-born: lineage points at the creator itself.
            (creator.content.clone(), Some(creator.id.clone()))
        } else {
            let variant = self
                .select_one(CandidateQuery {
                    n: DEFAULT_SELECT_N,
                    source_id: Some(creator.id.clone()),
                    min_new: 0,
                })
                .await?;
            // select_one 命中已记录 use_count/last_used_at（活跃信号），
            // 无需在此重复计数。
            (variant.content.clone(), Some(variant.id.clone()))
        };
        let user_prompt = self.create_neuron_user_prompt(&input, count, link_to)?;
        let drafts = self
            .generate_drafts(&prompt_content, &user_prompt, count, &[])
            .await?;
        let mut created = Vec::with_capacity(drafts.len());
        for draft in drafts {
            let create = NeuronCreate {
                desc: draft.desc,
                content: draft.content,
                weight: 0.0,
                system_type: None,
                tool_ids: draft.tool_ids,
                lineage_parent_id: lineage_parent_id.clone(),
                variant_state: None,
            };
            created.push(self.persist_plain(create, link_to)?);
        }
        Ok(created)
    }

    /// Ensure a system prompt root (any system_type). Idempotent unless `opts.reset`.
    pub async fn ensure_system_neuron(
        &self,
        system_type: &str,
        opts: EnsureSystemOpts,
    ) -> AppResult<Neuron> {
        let system_type = system_type.trim();
        if system_type.is_empty() {
            return Err(AppError::InvalidInput("system_type cannot be empty".into()));
        }

        tracing::info!(
            phase = "ensure_system_neuron",
            system_type,
            reset = opts.reset,
            "ensure_system_neuron start"
        );

        if opts.reset {
            if let Some(existing) = self.get_by_system_type(system_type)? {
                let _ = self.store()?.unlink_all_edges_of(&existing.id)?;
                let _ = self.delete_for_admin(&existing.id)?;
                tracing::info!(
                    phase = "ensure_system_neuron",
                    system_type,
                    neuron_id = %existing.id,
                    "reset deleted existing system neuron"
                );
            }
        } else if let Some(existing) = self.get_by_system_type(system_type)? {
            tracing::info!(
                phase = "ensure_system_neuron",
                system_type,
                neuron_id = %existing.id,
                "ensure_system_neuron hit existing; filling own downstream pool"
            );
            self.ensure_own_candidate_pool(&existing.id).await?;
            return Ok(existing);
        }

        let creator = self.ensure_creator()?;
        let tools_note = self.available_tools_block();
        let user_prompt = format!(
            "Write a system prompt neuron with system_type={system_type}.\n\
             Requirements:\n\
             - `content` must be a full executable system prompt: role, decision criteria, steps, output contract, hard constraints.\n\
             - Prefer 200–800 Chinese characters (or equivalent); no slogans or placeholders.\n\
             - One responsibility aligned with system_type={system_type}.\n\
             - Do not assign importance scores; system forces initial weight to 0.\n\
             - `tool_ids`: only truly needed tools; else [].\n\
             - {tools_note}\n\
             - Return ONLY JSON with desc, content, and tool_ids (weight optional/ignored)."
        );
        tracing::info!(
            phase = "ensure_system_neuron",
            system_type,
            step = "generate_draft",
            "generating system neuron draft from creator seed"
        );
        let draft = match self.generate_draft(&creator.content, &user_prompt).await {
            Ok(draft) => draft,
            Err(error) => {
                tracing::error!(
                    phase = "ensure_system_neuron",
                    system_type,
                    step = "generate_draft",
                    error_code = error.code(),
                    error = %error,
                    "generate_draft failed"
                );
                return Err(error);
            }
        };
        let created = self.persist_system_root(NeuronCreate {
            desc: if draft.desc.trim().is_empty() {
                system_type.to_string()
            } else {
                draft.desc
            },
            content: draft.content,
            weight: 0.0,
            system_type: Some(system_type.to_string()),
            tool_ids: draft.tool_ids,
            lineage_parent_id: None,
            variant_state: None,
        })?;
        tracing::info!(
            phase = "ensure_system_neuron",
            system_type,
            neuron_id = %created.id,
            "ensure_system_neuron created; filling own downstream pool"
        );
        self.ensure_own_candidate_pool(&created.id).await?;
        Ok(created)
    }

    /// Startup readiness: creator + selector only.
    pub async fn bootstrap(&self) -> AppResult<BootstrapReport> {
        tracing::info!(phase = "bootstrap", "bootstrap start");
        let creator = self.ensure_creator()?;
        // First-boot: ensure the creator owns its candidate pool (7 active slots).
        self.ensure_own_candidate_pool(&creator.id).await?;
        let selector = match self
            .ensure_system_neuron(ASSISTANT_SELECT_NEURON, EnsureSystemOpts { reset: false })
            .await
        {
            Ok(neuron) => neuron,
            Err(error) => {
                tracing::error!(
                    phase = "bootstrap",
                    error_code = error.code(),
                    error = %error,
                    "bootstrap failed at assistant_select_neuron"
                );
                return Err(error);
            }
        };
        tracing::info!(
            phase = "bootstrap",
            create_neuron_id = %creator.id,
            select_neuron_id = %selector.id,
            "bootstrap ok"
        );
        Ok(BootstrapReport {
            create_neuron_id: creator.id,
            select_neuron_id: selector.id,
        })
    }

    /// Ops: reset+recreate all known Assistant system prompts, then bootstrap.
    /// Does not reset `create_neuron` seed.
    pub async fn rebootstrap(&self) -> AppResult<BootstrapReport> {
        tracing::info!(phase = "rebootstrap", "rebootstrap start");
        let _ = self.ensure_creator()?;
        for system_type in REBOOTSTRAP_SYSTEM_TYPES {
            tracing::info!(
                phase = "rebootstrap",
                system_type,
                "resetting system prompt"
            );
            self.ensure_system_neuron(system_type, EnsureSystemOpts { reset: true })
                .await?;
        }
        let report = self.bootstrap().await?;
        tracing::info!(
            phase = "rebootstrap",
            create_neuron_id = %report.create_neuron_id,
            select_neuron_id = %report.select_neuron_id,
            "rebootstrap ok"
        );
        Ok(report)
    }

    pub fn ensure_creator(&self) -> AppResult<Neuron> {
        let mut cached_id = self.creator_id.lock().map_err(lock_error)?;
        if let Some(id) = cached_id.clone() {
            if let Some(neuron) = self.get(&id)? {
                if neuron.system_type.as_deref() == Some(CREATOR_SYSTEM_TYPE) {
                    tracing::debug!(
                        phase = "ensure_creator",
                        neuron_id = %neuron.id,
                        "creator cache hit"
                    );
                    return Ok(neuron);
                }
            }
            *cached_id = None;
        }

        if let Some(neuron) = self.store()?.get_neuron_by_system_type(CREATOR_SYSTEM_TYPE)? {
            *cached_id = Some(neuron.id.clone());
            tracing::info!(
                phase = "ensure_creator",
                neuron_id = %neuron.id,
                "creator loaded from store"
            );
            return Ok(neuron);
        }

        let prompt = self.config.create_neuron_prompt()?;
        let neuron = self.store()?.create_neuron(NeuronCreate {
            desc: "创建神经元".into(),
            content: prompt,
            weight: 0.0,
            system_type: Some(CREATOR_SYSTEM_TYPE.into()),
            tool_ids: Vec::new(),
            lineage_parent_id: None,
            variant_state: None,
        })?;
        *cached_id = Some(neuron.id.clone());
        tracing::info!(
            phase = "ensure_creator",
            neuron_id = %neuron.id,
            "creator created from seed"
        );
        Ok(neuron)
    }

    // ── Creator self-iteration ─────────────────────────────────

    /// Bump `use_count` / `last_used_at` for a variant that was just used to
    /// generate a child neuron.
    pub fn record_variant_usage(&self, variant_id: &str) -> AppResult<Neuron> {
        self.store()?.increment_variant_usage(variant_id)
    }

    /// Accumulate a signed score delta onto a variant (lineage attribution).
    pub fn accumulate_variant_delta(&self, variant_id: &str, delta: f64) -> AppResult<Neuron> {
        self.store()?.accumulate_variant_delta(variant_id, delta)
    }

    /// Evaluate the creator variant pool after a score feedback round.
    /// Steps (each call acts on at most ONE variant):
    /// 1. Observing slots: promote when `use_count >= 1`, rollback when delta < 0.
    /// 2. Elimination candidates (delta <= -3, or use_count >= 10 with delta < 0).
    /// 3. Rewrite candidates (use_count >= 3 and |delta| >= 2): differential rewrite.
    pub async fn maybe_evolve_creator_variants(&self) -> AppResult<()> {
        let creator = self.ensure_creator()?;
        let variants = self.store()?.get_variants(&creator.id, false)?;
        if variants.is_empty() {
            return Ok(());
        }

        // 1. Observing slots.
        for variant in variants
            .iter()
            .filter(|v| v.variant_state.as_deref() == Some("observing"))
        {
            if variant.accumulated_delta < 0.0 {
                self.rollback_variant_if_regressed(&variant.neuron.id)?;
                return Ok(());
            }
            if variant.use_count >= 1 {
                self.store()?
                    .set_variant_state(&variant.neuron.id, Some("active"))?;
                tracing::info!(
                    phase = "maybe_evolve_creator_variants",
                    variant_id = %variant.neuron.id,
                    use_count = variant.use_count,
                    "observing variant promoted to active"
                );
                return Ok(());
            }
        }

        // 2. Elimination candidates.
        for variant in variants
            .iter()
            .filter(|v| v.variant_state.as_deref() != Some("observing"))
        {
            if variant.manual_edited {
                continue;
            }
            let eliminated = variant.accumulated_delta <= -3.0
                || (variant.use_count >= 10 && variant.accumulated_delta < 0.0);
            if eliminated {
                self.rollback_variant_if_regressed(&variant.neuron.id)?;
                tracing::info!(
                    phase = "maybe_evolve_creator_variants",
                    variant_id = %variant.neuron.id,
                    accumulated_delta = variant.accumulated_delta,
                    use_count = variant.use_count,
                    "variant eliminated; rolling back"
                );
                return Ok(());
            }
        }

        // 3. Rewrite candidate (at most one per call).
        for variant in variants
            .iter()
            .filter(|v| v.variant_state.as_deref() != Some("observing"))
        {
            if variant.manual_edited {
                continue;
            }
            if variant.use_count >= 3 && variant.accumulated_delta.abs() >= 2.0 {
                match self.rewrite_variant(&creator, variant).await {
                    Ok(()) => {
                        tracing::info!(
                            phase = "maybe_evolve_creator_variants",
                            variant_id = %variant.neuron.id,
                            "variant differentially rewritten; moved to observing"
                        );
                        return Ok(());
                    }
                    Err(error) => {
                        // Failure keeps the old version; never blocks the create flow.
                        tracing::warn!(
                            phase = "maybe_evolve_creator_variants",
                            variant_id = %variant.neuron.id,
                            error = %error,
                            "rewrite failed; keeping old version"
                        );
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    /// Restore a variant to its most recent archived version.
    /// With no history, demote the variant to observing so it exits the active pool.
    fn rollback_variant_if_regressed(&self, variant_id: &str) -> AppResult<()> {
        let store = self.store()?;
        let Some(version) = store.latest_version_of(variant_id)? else {
            store.set_variant_state(variant_id, Some("observing"))?;
            tracing::info!(
                phase = "rollback_variant_if_regressed",
                variant_id,
                "no archived version; demoted to observing"
            );
            return Ok(());
        };
        store.update_neuron(
            variant_id,
            NeuronUpdate {
                desc: None,
                content: Some(version.content.clone()),
                ..Default::default()
            },
        )?;
        store.insert_neuron_version(
            variant_id,
            &version.content,
            "rollback",
            Some(&version.id),
        )?;
        store.set_variant_state(variant_id, Some("active"))?;
        tracing::info!(
            phase = "rollback_variant_if_regressed",
            variant_id,
            version_id = %version.id,
            "rolled back to archived version"
        );
        Ok(())
    }

    /// Differential rewrite of one variant via the `creator.variant_evolve` contract.
    /// On success the variant is updated with the new content and moved to the
    /// observing slot; the previous content is archived in `neuron_versions`.
    async fn rewrite_variant(&self, creator: &Neuron, variant: &NeuronVariant) -> AppResult<()> {
        let payload = serde_json::json!({
            "current_desc": variant.neuron.desc,
            "current_content": variant.neuron.content,
            "current_tool_ids": variant.neuron.tool_ids,
            "use_count": variant.use_count,
            "accumulated_delta": variant.accumulated_delta,
            "last_used_at": variant.last_used_at,
            "parent_creator_content": creator.content,
        });
        let insert = InsertCatalog::require("creator.variant_evolve");
        let messages = ModelCallInput::assemble(
            &[],
            &creator.content,
            insert,
            &payload.to_string(),
            ModelAppendTemplate::Manual,
        );
        let output = self.model_caller.call_model(messages).await?;
        let decision = extract_json_object(&output)?;
        let desc = decision
            .get("desc")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| variant.neuron.desc.clone());
        let content = decision
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("rewrite response missing content".into()))?;

        let store = self.store()?;
        // Archive the current version before mutating.
        let prev = store.latest_version_of(&variant.neuron.id)?;
        store.insert_neuron_version(
            &variant.neuron.id,
            &variant.neuron.content,
            "evolve",
            prev.as_ref().map(|p| p.id.as_str()),
        )?;
        store.update_neuron(
            &variant.neuron.id,
            NeuronUpdate {
                desc: Some(desc),
                content: Some(content.to_string()),
                ..Default::default()
            },
        )?;
        store.set_variant_state(&variant.neuron.id, Some("observing"))?;
        Ok(())
    }

    fn resolve_source_id(&self, source_id: Option<&str>) -> AppResult<Option<String>> {
        let Some(source_id) = source_id else {
            return Ok(None);
        };
        if self.get_neuron(source_id)?.is_none() {
            return Err(AppError::NeuronNotFound(source_id.to_string()));
        }
        Ok(Some(source_id.to_string()))
    }

    async fn try_llm_select(
        &self,
        candidates: &[Neuron],
        history: &[ModelMessage],
    ) -> AppResult<Neuron> {
        let selector = self
            .get_neuron_by_system_type(ASSISTANT_SELECT_NEURON)?
            .ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "Missing system prompt neuron with system_type={ASSISTANT_SELECT_NEURON}"
                ))
            })?;
        let payload = serde_json::json!({
            "candidates": candidates.iter().map(|n| serde_json::json!({
                "id": n.id,
                "desc": n.desc,
                "content": n.content,
                "weight": n.weight,
                "tool_ids": n.tool_ids,
            })).collect::<Vec<_>>(),
        });
        // Append subject is the select-one manual; neuron content stays in role_system.
        let insert = InsertCatalog::require("neuron.select_one");
        let messages = ModelCallInput::assemble(
            history,
            &selector.content,
            insert,
            &payload.to_string(),
            ModelAppendTemplate::Manual,
        );
        tracing::info!(
            phase = "select_neuron.model_input",
            selector_id = %selector.id,
            system_prompt_len = selector.content.len(),
            insert_id = "neuron.select_one",
            history_len = history.len(),
            candidate_payload = %payload,
            "select_neuron model input assembled"
        );
        tracing::debug!(
            phase = "select_neuron.model_input.full",
            system_prompt = %selector.content,
            insert_text = %insert,
            "select_neuron full model input (debug)"
        );
        let output = self.model_caller.call_model(messages).await?;
        tracing::info!(
            phase = "select_neuron.model_output",
            raw_output = %output,
            "select_neuron model raw output"
        );
        let decision = extract_json_object(&output)?;
        let neuron_id = decision
            .get("neuron_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::InvalidInput("select neuron response missing neuron_id".into())
            })?;
        tracing::info!(
            phase = "select_neuron.model_decision",
            neuron_id = %neuron_id,
            in_candidates = candidates.iter().any(|n| n.id == neuron_id),
            "select_neuron llm decision"
        );
        candidates
            .iter()
            .find(|n| n.id == neuron_id)
            .cloned()
            .ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "Selected neuron_id {neuron_id} is not in candidates"
                ))
            })
    }

    async fn generate_draft(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> AppResult<GeneratedNeuronDraft> {
        let mut drafts = self
            .generate_drafts(system_prompt, user_prompt, 1, &[])
            .await?;
        drafts.pop().ok_or_else(|| {
            AppError::NeuronBootstrapFailed("Generated neuron list was empty".into())
        })
    }

    async fn generate_drafts(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        expected: usize,
        history: &[ModelMessage],
    ) -> AppResult<Vec<GeneratedNeuronDraft>> {
        // Append subject is the draft manual; creator/prompt neuron stays in role_system.
        let insert = InsertCatalog::require("neuron.draft_from_model");
        let messages = ModelCallInput::assemble(
            history,
            system_prompt,
            insert,
            user_prompt,
            ModelAppendTemplate::Manual,
        );
        tracing::info!(
            phase = "generate_drafts",
            message_count = messages.len(),
            user_len = user_prompt.len(),
            expected,
            "generate_drafts model call start"
        );
        let output = match self.model_caller.call_model(messages).await {
            Ok(output) => output,
            Err(error) => {
                tracing::error!(
                    phase = "generate_drafts",
                    error_code = error.code(),
                    error = %error,
                    "generate_drafts model call failed"
                );
                return Err(error);
            }
        };
        let drafts = parse_generated_drafts(&output, expected).map_err(|error| {
            tracing::error!(
                phase = "generate_drafts",
                error = %error,
                "generate_drafts JSON parse failed"
            );
            error
        })?;
        tracing::info!(
            phase = "generate_drafts",
            count = drafts.len(),
            "generate_drafts ok"
        );
        Ok(drafts)
    }

    /// Fill `count` ordinary neurons under `source_id` in one model call (create-flow guts).
    /// Does not call `select_one` / `create_neuron` — avoids async recursion when creator pool is empty.
    async fn fill_candidates_batch(
        &self,
        source_id: Option<&str>,
        count: usize,
    ) -> AppResult<Vec<Neuron>> {
        if count == 0 {
            return Ok(Vec::new());
        }
        if count > MAX_CREATE_NEURON_COUNT {
            return Err(AppError::InvalidInput(format!(
                "fill batch count must be 1..={MAX_CREATE_NEURON_COUNT}, got {count}"
            )));
        }
        tracing::info!(
            phase = "fill_candidates_batch",
            source_id = source_id.unwrap_or(""),
            count,
            "fill_candidates_batch start"
        );
        let creator = self.ensure_creator()?;
        // Prefer an existing creator-child as write prompt; never select_one (would re-enter fill).
        let prompt_content = {
            let existing = self.store()?.list_direct_downstream(
                &creator.id,
                DEFAULT_SELECT_N,
                &HashSet::new(),
            )?;
            if existing.is_empty() {
                creator.content.clone()
            } else {
                pick_by_weight(&existing)?.content
            }
        };
        let purpose = match source_id {
            Some(source_id) => format!(
                "Fill {count} distinct single-responsibility downstream neurons under source_id {source_id}. \
                 Specialize useful child capabilities of the source; do not duplicate the parent wholesale."
            ),
            None => format!(
                "Fill {count} distinct single-responsibility neurons for a global candidate pool."
            ),
        };
        let user_prompt = self.create_neuron_user_prompt(
            &CreateNeuronInput::Purpose(purpose),
            count,
            source_id,
        )?;
        let drafts = self
            .generate_drafts(&prompt_content, &user_prompt, count, &[])
            .await?;
        let mut created = Vec::with_capacity(drafts.len());
        for draft in drafts {
            created.push(self.persist_plain(
                NeuronCreate {
                    desc: draft.desc,
                    content: draft.content,
                    weight: 0.0,
                    system_type: None,
                    tool_ids: draft.tool_ids,
                    lineage_parent_id: None,
                    variant_state: None,
                },
                source_id,
            )?);
        }
        Ok(created)
    }

    async fn ensure_own_candidate_pool(&self, root_id: &str) -> AppResult<()> {
        let _ = self
            .select_candidates(CandidateQuery {
                n: DEFAULT_SELECT_N,
                source_id: Some(root_id.to_string()),
                min_new: 0,
            })
            .await?;
        Ok(())
    }

    fn create_neuron_user_prompt(
        &self,
        input: &CreateNeuronInput,
        count: usize,
        link_to: Option<&str>,
    ) -> AppResult<String> {
        let count_word = if count == 1 {
            "exactly 1".to_string()
        } else {
            format!("exactly {count}")
        };
        let list_contract = if count == 1 {
            "Return ONLY a JSON array with exactly 1 object: \
             [{\"desc\",\"content\",\"tool_ids\"}] (weight optional/ignored). \
             A single object is also accepted for count=1."
                .to_string()
        } else {
            format!(
                "Return ONLY a JSON array with exactly {count} objects: \
                 [{{\"desc\",\"content\",\"tool_ids\"}}, ...] (weight optional/ignored). \
                 Each neuron must be distinct and single-responsibility."
            )
        };
        let link_note = match link_to {
            Some(id) => format!(" These neurons will be direct downstream of {id}."),
            None => String::new(),
        };
        let tools_note = self.available_tools_block();
        Ok(match input {
            CreateNeuronInput::Purpose(purpose) => format!(
                "Create {count_word} single-responsibility neuron(s) for the purpose below.{link_note}\n\
                 Requirements:\n\
                 - Each neuron focuses on one job only; do not bundle unrelated skills.\n\
                 - `content` must be an executable prompt/knowledge block (role, when to use / not use, steps, output format, hard constraints).\n\
                 - Prefer 200–800 Chinese characters (or equivalent) in `content`; no slogans or placeholders.\n\
                 - Do not assign importance scores; system forces initial weight to 0.\n\
                 - `tool_ids`: only truly needed tools; else [].\n\
                 - {tools_note}\n\
                 - {list_contract}\n\
                 Purpose: {purpose}"
            ),
            CreateNeuronInput::Messages(messages) => format!(
                "Create {count_word} single-responsibility neuron(s) distilled from the conversation context below.{link_note}\n\
                 Requirements:\n\
                 - Infer reusable capabilities the conversation needs; ignore one-off chatter.\n\
                 - Each neuron focuses on one job only; do not bundle unrelated skills.\n\
                 - `content` must be an executable prompt/knowledge block (role, when to use / not use, steps, output format, hard constraints).\n\
                 - Prefer 200–800 Chinese characters (or equivalent) in `content`; no slogans or placeholders.\n\
                 - Do not assign importance scores; system forces initial weight to 0.\n\
                 - `tool_ids`: only truly needed tools; else [].\n\
                 - {tools_note}\n\
                 - {list_contract}\n\
                 Context: {}",
                serde_json::to_string(messages).unwrap_or_default()
            ),
        })
    }

    /// 前端手动创建：store 直持久化，不触发 LLM 草稿生成。
    /// link_to = None => 孤立神经元；Some(id) => 该神经元的下游神经元（自动建边，边权重 0）。
    pub fn create_plain(
        &self,
        create: NeuronCreate,
        link_to: Option<&str>,
    ) -> AppResult<Neuron> {
        self.persist_plain(create, link_to)
    }

    fn persist_plain(
        &self,
        mut create: NeuronCreate,
        link_to: Option<&str>,
    ) -> AppResult<Neuron> {
        create.system_type = None;
        create.weight = 0.0;
        match link_to {
            Some(source_id) => self
                .store()?
                .create_downstream_neuron(source_id, create, 0.0)
                .map(|(neuron, _)| neuron),
            None => self.store()?.create_neuron(create),
        }
    }

    fn persist_system_root(&self, mut create: NeuronCreate) -> AppResult<Neuron> {
        if create.system_type.as_deref().unwrap_or("").trim().is_empty() {
            return Err(AppError::InvalidInput(
                "persist_system_root requires system_type".into(),
            ));
        }
        create.weight = 0.0;
        self.store()?.create_neuron(create)
    }

    fn store(&self) -> AppResult<std::sync::MutexGuard<'_, NeuronStore>> {
        self.store.lock().map_err(lock_error)
    }

    /// 活跃数据超容量时，按低价值排序回收最低价值节点（逻辑删除），返回回收数量。
    /// 系统提示词（system_type IS NOT NULL）豁免；幂等，未超容量时返回 0。
    pub fn recycle_if_over_capacity(&self) -> AppResult<usize> {
        let capacity = self.config.capacity()?;
        let store = self.store()?;
        let active = store.count_active()?;
        let over = active.saturating_sub(capacity);
        if over == 0 {
            return Ok(0);
        }
        let victims = store.select_low_value(over)?;
        if victims.is_empty() {
            return Ok(0);
        }
        let recycled = store.mark_deleted(&victims)?;
        tracing::info!(
            phase = "recycle_if_over_capacity",
            capacity,
            active_before = active,
            victims = victims.len(),
            recycled,
            "recycled low-value neurons over capacity"
        );
        Ok(recycled)
    }
}

fn pick_by_weight(candidates: &[Neuron]) -> AppResult<Neuron> {
    let max_weight = candidates
        .iter()
        .map(|n| n.weight)
        .fold(f64::NEG_INFINITY, f64::max);
    let tops: Vec<&Neuron> = candidates
        .iter()
        .filter(|n| (n.weight - max_weight).abs() < f64::EPSILON || n.weight == max_weight)
        .collect();
    if tops.is_empty() {
        return Err(AppError::InvalidInput(
            "No neuron candidates available for selection".into(),
        ));
    }
    let idx = (now_ms() as usize).wrapping_mul(2654435761) % tops.len();
    Ok(tops[idx].clone())
}

fn extract_json_object(text: &str) -> AppResult<serde_json::Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if value.is_object() {
            return Ok(value);
        }
    }
    let start = trimmed
        .find('{')
        .ok_or_else(|| AppError::InvalidInput("LLM response missing JSON object".into()))?;
    let end = trimmed
        .rfind('}')
        .ok_or_else(|| AppError::InvalidInput("LLM response missing JSON object end".into()))?;
    if end < start {
        return Err(AppError::InvalidInput(
            "LLM response has invalid JSON object bounds".into(),
        ));
    }
    serde_json::from_str(&trimmed[start..=end])
        .map_err(|e| AppError::InvalidInput(format!("Failed to parse LLM JSON: {e}")))
}

fn extract_json_array(text: &str) -> AppResult<serde_json::Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if value.is_array() {
            return Ok(value);
        }
        if let Some(neurons) = value.get("neurons").filter(|v| v.is_array()) {
            return Ok(neurons.clone());
        }
    }
    let start = trimmed
        .find('[')
        .ok_or_else(|| AppError::InvalidInput("LLM response missing JSON array".into()))?;
    let end = trimmed
        .rfind(']')
        .ok_or_else(|| AppError::InvalidInput("LLM response missing JSON array end".into()))?;
    if end < start {
        return Err(AppError::InvalidInput(
            "LLM response has invalid JSON array bounds".into(),
        ));
    }
    serde_json::from_str(&trimmed[start..=end])
        .map_err(|e| AppError::InvalidInput(format!("Failed to parse LLM JSON array: {e}")))
}

fn parse_generated_drafts(text: &str, expected: usize) -> AppResult<Vec<GeneratedNeuronDraft>> {
    if expected == 0 {
        return Err(AppError::InvalidInput(
            "expected draft count must be >= 1".into(),
        ));
    }

    let trimmed = text.trim();
    let mut drafts: Vec<GeneratedNeuronDraft> = Vec::new();

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        drafts = drafts_from_json_value(value)?;
    }
    // Prefer a real list before falling back; ignore empty `[]` false-positives
    // (e.g. `"tool_ids":[]` inside a single object).
    if drafts.is_empty() {
        if let Ok(array) = extract_json_array(trimmed) {
            drafts = drafts_from_json_value(array)?;
        }
    }
    if drafts.is_empty() && expected == 1 {
        let value = extract_json_object(trimmed)?;
        drafts = drafts_from_json_value(value)?;
    }
    if drafts.is_empty() {
        return Err(AppError::NeuronBootstrapFailed(
            "Generated neuron list was empty".into(),
        ));
    }
    if drafts.len() > expected {
        drafts.truncate(expected);
    }
    if drafts.len() != expected {
        return Err(AppError::NeuronBootstrapFailed(format!(
            "Expected {expected} generated neuron(s), got {}",
            drafts.len()
        )));
    }
    for draft in &drafts {
        if draft.desc.trim().is_empty() || draft.content.trim().is_empty() {
            return Err(AppError::NeuronBootstrapFailed(
                "Generated neuron must have non-empty desc/content".into(),
            ));
        }
    }
    Ok(drafts)
}

fn drafts_from_json_value(value: serde_json::Value) -> AppResult<Vec<GeneratedNeuronDraft>> {
    match value {
        serde_json::Value::Array(items) => serde_json::from_value(serde_json::Value::Array(items))
            .map_err(|error| {
                AppError::NeuronBootstrapFailed(format!(
                    "Invalid generated neuron list JSON: {error}"
                ))
            }),
        serde_json::Value::Object(map) => {
            if let Some(neurons) = map.get("neurons").cloned() {
                if neurons.is_array() {
                    return drafts_from_json_value(neurons);
                }
            }
            let draft: GeneratedNeuronDraft =
                serde_json::from_value(serde_json::Value::Object(map)).map_err(|error| {
                    AppError::NeuronBootstrapFailed(format!(
                        "Invalid generated neuron JSON: {error}"
                    ))
                })?;
            Ok(vec![draft])
        }
        _ => Ok(Vec::new()),
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn lock_error<T: std::fmt::Display>(error: T) -> AppError {
    AppError::StorageError(format!("Lock error: {error}"))
}

// AI tool adapters retained but unregistered until inserts exist.
#[allow(dead_code)]
struct GetNeuronTool {
    manager: Arc<NeuronManager>,
}

#[allow(dead_code)]
impl GetNeuronTool {
    fn new(manager: Arc<NeuronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for GetNeuronTool {
    fn name(&self) -> &str {
        "get_neuron"
    }

    fn description(&self) -> &str {
        "Get details of a neuron by ID, including its connections"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {"id": {"type": "string", "description": "Neuron ID"}},
            "required": ["id"]
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = required_str(&args, "id")?;
        let neuron = self
            .manager
            .get_neuron(id)?
            .ok_or_else(|| AppError::NeuronNotFound(id.to_string()))?;
        let connections = self.manager.get_connections(id)?;
        Ok(format!(
            "{}\nConnections: {}",
            serde_json::to_string(&neuron).map_err(|e| AppError::StorageError(e.to_string()))?,
            serde_json::to_string(&connections)
                .map_err(|e| AppError::StorageError(e.to_string()))?
        ))
    }
}

#[allow(dead_code)]
struct ListNeuronsTool {
    manager: Arc<NeuronManager>,
}

#[allow(dead_code)]
impl ListNeuronsTool {
    fn new(manager: Arc<NeuronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for ListNeuronsTool {
    fn name(&self) -> &str {
        "list_neurons"
    }

    fn description(&self) -> &str {
        "List all neurons"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn execute(&self, _args: Value) -> AppResult<String> {
        serde_json::to_string(&self.manager.list_neurons()?)
            .map_err(|e| AppError::StorageError(e.to_string()))
    }
}

#[allow(dead_code)]
struct UpdateNeuronTool {
    manager: Arc<NeuronManager>,
}

#[allow(dead_code)]
impl UpdateNeuronTool {
    fn new(manager: Arc<NeuronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for UpdateNeuronTool {
    fn name(&self) -> &str {
        "update_neuron"
    }

    fn description(&self) -> &str {
        "Update a regular neuron's description or content"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "desc": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = required_str(&args, "id")?;
        let update = NeuronUpdate {
            desc: args.get("desc").and_then(Value::as_str).map(str::to_string),
            content: args
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_string),
            tool_ids: args
                .get("tool_ids")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                }),
        };
        let neuron = self.manager.update_content_for_ai(id, update)?;
        serde_json::to_string(&neuron).map_err(|e| AppError::StorageError(e.to_string()))
    }
}

#[allow(dead_code)]
struct GetNetworkTool {
    manager: Arc<NeuronManager>,
}

#[allow(dead_code)]
impl GetNetworkTool {
    fn new(manager: Arc<NeuronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for GetNetworkTool {
    fn name(&self) -> &str {
        "get_network"
    }

    fn description(&self) -> &str {
        "BFS traverse the neuron network from a seed up to max_depth; returns { seed_id, neurons, connections }"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "max_depth": {"type": "integer", "minimum": 0, "default": 3}
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = required_str(&args, "id")?;
        let max_depth = args.get("max_depth").and_then(Value::as_u64).unwrap_or(3) as usize;
        serde_json::to_string(&self.manager.get_network(id, max_depth)?)
            .map_err(|e| AppError::StorageError(e.to_string()))
    }
}

#[allow(dead_code)]
struct CreateNeuronTool {
    manager: Arc<NeuronManager>,
}

#[allow(dead_code)]
impl CreateNeuronTool {
    fn new(manager: Arc<NeuronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for CreateNeuronTool {
    fn name(&self) -> &str {
        "create_neuron"
    }

    fn description(&self) -> &str {
        "Create 1..=10 regular neurons via the unified creation flow (pool→7→1). Model returns a list. Optionally link all as direct downstream of source_id."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "purpose": {"type": "string", "description": "Purpose for the new neuron(s)"},
                "count": {"type": "integer", "minimum": 1, "maximum": 10, "default": 1},
                "source_id": {"type": "string", "description": "Optional parent to link as direct downstream"}
            },
            "required": ["purpose"]
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let purpose = required_str(&args, "purpose")?;
        let link_to = args.get("source_id").and_then(Value::as_str);
        let count = args.get("count").and_then(Value::as_u64).unwrap_or(1) as usize;
        let neurons = self
            .manager
            .create_neuron(
                CreateNeuronInput::Purpose(purpose.to_string()),
                link_to,
                count,
            )
            .await?;
        serde_json::to_string(&neurons).map_err(|e| AppError::StorageError(e.to_string()))
    }
}

#[allow(dead_code)]
struct SelectNeuronCandidatesTool {
    manager: Arc<NeuronManager>,
}

#[allow(dead_code)]
impl SelectNeuronCandidatesTool {
    fn new(manager: Arc<NeuronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for SelectNeuronCandidatesTool {
    fn name(&self) -> &str {
        "select_neuron_candidates"
    }

    fn description(&self) -> &str {
        "Select high-weight neuron candidates and create missing candidates"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "n": {"type": "integer", "minimum": 0},
                "source_id": {"type": "string"},
                "min_new": {"type": "integer", "minimum": 0, "default": 0}
            },
            "required": ["n"]
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let n = args
            .get("n")
            .and_then(Value::as_u64)
            .ok_or_else(|| AppError::InvalidInput("Missing or invalid: n".into()))?
            as usize;
        let query = CandidateQuery {
            n,
            source_id: args
                .get("source_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            min_new: args.get("min_new").and_then(Value::as_u64).unwrap_or(0) as usize,
        };
        serde_json::to_string(&self.manager.select_candidates(query).await?)
            .map_err(|e| AppError::StorageError(e.to_string()))
    }
}

#[allow(dead_code)]
fn required_str<'a>(args: &'a Value, key: &str) -> AppResult<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::InvalidInput(format!("Missing: {key}")))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use rusqlite::Connection as SqliteConnection;

    use super::*;
    use crate::core::models::ModelMessageRole;

    struct MockModelCaller {
        calls: AtomicUsize,
    }

    fn prompt_blob_from_messages(messages: &[ModelMessage]) -> String {
        messages
            .iter()
            .filter(|m| {
                matches!(
                    m.role,
                    ModelMessageRole::System | ModelMessageRole::User
                )
            })
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[async_trait]
    impl NeuronModelCaller for MockModelCaller {
        async fn call_model(&self, messages: Vec<ModelMessage>) -> AppResult<String> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            let user_prompt = prompt_blob_from_messages(&messages);
            let count = user_prompt
                .split("exactly ")
                .nth(1)
                .and_then(|rest| {
                    rest.split_whitespace()
                        .next()
                        .and_then(|token| token.trim_matches(|c: char| !c.is_ascii_digit()).parse().ok())
                })
                .unwrap_or(1usize);
            if count <= 1 {
                return Ok(format!(
                    r#"{{"desc":"generated-{call}","content":"content-{call}","weight":1.0,"tool_ids":[]}}"#
                ));
            }
            let items: Vec<String> = (0..count)
                .map(|i| {
                    format!(
                        r#"{{"desc":"generated-{call}-{i}","content":"content-{call}-{i}","weight":1.0,"tool_ids":[]}}"#
                    )
                })
                .collect();
            Ok(format!("[{}]", items.join(",")))
        }
    }

    fn test_manager() -> (Arc<NeuronManager>, std::path::PathBuf) {
        let conn = Arc::new(Mutex::new(SqliteConnection::open_in_memory().unwrap()));
        let store = Arc::new(Mutex::new(NeuronStore::new(conn)));
        store.lock().unwrap().init_table().unwrap();
        let root = std::env::temp_dir().join(format!(
            "agent-app-neuron-manager-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("config.json"),
            r#"{"neurons":{"bootstrap":{"create_neuron_prompt":"create a neuron"}}}"#,
        )
        .unwrap();
        let manager = Arc::new(NeuronManager::new(
            store,
            Arc::new(MockModelCaller {
                calls: AtomicUsize::new(0),
            }),
            NeuronConfigReader::new(root.clone()),
            Arc::new(RwLock::new(ToolRegistry::new())),
        ));
        (manager, root)
    }

    fn insert_plain(manager: &NeuronManager, desc: &str, content: &str) -> Neuron {
        manager
            .store()
            .unwrap()
            .create_neuron(NeuronCreate {
                desc: desc.into(),
                content: content.into(),
                ..Default::default()
            })
            .unwrap()
    }

    fn insert_downstream(manager: &NeuronManager, parent_id: &str, desc: &str) -> Neuron {
        manager
            .create_plain(
                NeuronCreate {
                    desc: desc.into(),
                    content: format!("{desc} content"),
                    ..Default::default()
                },
                Some(parent_id),
            )
            .unwrap()
    }

    #[tokio::test]
    async fn select_candidates_prefers_source_id_and_fills_to_n() {
        let (manager, root) = test_manager();
        let source = insert_plain(&manager, "source", "source content");
        let candidates = manager
            .select_candidates(CandidateQuery {
                n: 3,
                source_id: Some(source.id.clone()),
                min_new: 2,
            })
            .await
            .unwrap();
        assert_eq!(candidates.len(), 3);
        let downstream = manager
            .store()
            .unwrap()
            .list_direct_downstream(&source.id, 10, &HashSet::new())
            .unwrap();
        assert_eq!(downstream.len(), 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_builds_children_self_siblings_and_three_ancestors() {
        let (manager, root) = test_manager();
        let great_grandparent = insert_plain(&manager, "great-grandparent", "great");
        let grandparent = insert_downstream(&manager, &great_grandparent.id, "grandparent");
        let parent = insert_downstream(&manager, &grandparent.id, "parent");
        let self_neuron = insert_downstream(&manager, &parent.id, "self");
        let sibling = insert_downstream(&manager, &parent.id, "sibling");
        let existing_child_a = insert_downstream(&manager, &self_neuron.id, "child-a");
        let existing_child_b = insert_downstream(&manager, &self_neuron.id, "child-b");

        let candidates = manager
            .select_assistant_candidates(AssistantCandidateScope::neighborhood_default(
                self_neuron.id.clone(),
            ))
            .await
            .unwrap();
        let candidate_ids: HashSet<&str> = candidates.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(candidate_ids.len(), candidates.len());
        assert!(candidate_ids.contains(self_neuron.id.as_str()));
        assert!(candidate_ids.contains(sibling.id.as_str()));
        assert!(candidate_ids.contains(parent.id.as_str()));
        assert!(candidate_ids.contains(grandparent.id.as_str()));
        assert!(candidate_ids.contains(great_grandparent.id.as_str()));
        assert!(candidate_ids.contains(existing_child_a.id.as_str()));
        assert!(candidate_ids.contains(existing_child_b.id.as_str()));

        let downstream = manager
            .store()
            .unwrap()
            .list_direct_downstream(&self_neuron.id, 20, &HashSet::new())
            .unwrap();
        assert_eq!(downstream.len(), 6);
        assert_eq!(candidates.len(), 11);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_adds_two_new_children_when_four_exist() {
        let (manager, root) = test_manager();
        let self_neuron = insert_plain(&manager, "self", "self");
        for index in 0..4 {
            insert_downstream(&manager, &self_neuron.id, &format!("child-{index}"));
        }

        let candidates = manager
            .select_assistant_candidates(AssistantCandidateScope::neighborhood_default(
                self_neuron.id.clone(),
            ))
            .await
            .unwrap();
        let downstream = manager
            .store()
            .unwrap()
            .list_direct_downstream(&self_neuron.id, 20, &HashSet::new())
            .unwrap();
        assert_eq!(downstream.len(), 6);
        assert_eq!(candidates.len(), 7);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_global_default_uses_existing_seven() {
        let (manager, root) = test_manager();
        for index in 0..7 {
            insert_plain(&manager, &format!("global-{index}"), "global");
        }
        let count_before = manager.list().unwrap().len();

        let candidates = manager
            .select_assistant_candidates(AssistantCandidateScope::global_default())
            .await
            .unwrap();

        assert_eq!(candidates.len(), DEFAULT_SELECT_N);
        assert_eq!(manager.list().unwrap().len(), count_before);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_honors_custom_neighborhood_policy() {
        let (manager, root) = test_manager();
        let self_neuron = insert_plain(&manager, "self", "self");
        let existing_child = insert_downstream(&manager, &self_neuron.id, "existing-child");
        let policy = NeighborhoodPoolPolicy {
            existing_downstream: 2,
            new_downstream: 1,
            fill_downstream_shortage: false,
            siblings: 0,
            upstream_depth: 0,
            global_top_weight: 0,
        };

        let candidates = manager
            .select_assistant_candidates(AssistantCandidateScope::Neighborhood {
                self_id: self_neuron.id.clone(),
                policy,
            })
            .await
            .unwrap();
        let candidate_ids: HashSet<&str> = candidates.iter().map(|n| n.id.as_str()).collect();
        let downstream = manager
            .store()
            .unwrap()
            .list_direct_downstream(&self_neuron.id, 20, &HashSet::new())
            .unwrap();

        assert_eq!(downstream.len(), 2);
        assert_eq!(candidates.len(), 3);
        assert!(candidate_ids.contains(self_neuron.id.as_str()));
        assert!(candidate_ids.contains(existing_child.id.as_str()));
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_appends_global_top_weight_neurons() {
        let (manager, root) = test_manager();
        let self_neuron = insert_plain(&manager, "self", "self");
        insert_downstream(&manager, &self_neuron.id, "child");
        // 高权重孤立节点：不在 self 邻域内，weight 远超其余节点，应被 top5 补充进池。
        let top = insert_plain(&manager, "top-weighted", "top");
        manager.adjust_weight(&top.id, 50.0).unwrap();

        let candidates = manager
            .select_assistant_candidates(AssistantCandidateScope::neighborhood_default(
                self_neuron.id.clone(),
            ))
            .await
            .unwrap();
        let ids: HashSet<&str> = candidates.iter().map(|n| n.id.as_str()).collect();
        assert!(
            ids.contains(top.id.as_str()),
            "global top weighted node must be appended to neighborhood pool"
        );
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_zero_top_weight_skips_global_append() {
        let (manager, root) = test_manager();
        let self_neuron = insert_plain(&manager, "self", "self");
        let top = insert_plain(&manager, "top-weighted", "top");
        manager.adjust_weight(&top.id, 50.0).unwrap();

        let candidates = manager
            .select_assistant_candidates(AssistantCandidateScope::Neighborhood {
                self_id: self_neuron.id.clone(),
                policy: NeighborhoodPoolPolicy {
                    existing_downstream: 0,
                    new_downstream: 1,
                    fill_downstream_shortage: false,
                    siblings: 0,
                    upstream_depth: 0,
                    global_top_weight: 0,
                },
            })
            .await
            .unwrap();
        let ids: HashSet<&str> = candidates.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(self_neuron.id.as_str()));
        assert!(
            !ids.contains(top.id.as_str()),
            "zero global_top_weight must not append top weighted node"
        );
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_rejects_zero_global_limit() {
        let (manager, root) = test_manager();
        let result = manager
            .select_assistant_candidates(AssistantCandidateScope::Global { limit: 0 })
            .await;
        assert!(result.is_err());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_rejects_new_downstream_above_batch_limit() {
        let (manager, root) = test_manager();
        let self_neuron = insert_plain(&manager, "self", "self");
        let result = manager
            .select_assistant_candidates(AssistantCandidateScope::Neighborhood {
                self_id: self_neuron.id,
                policy: NeighborhoodPoolPolicy {
                    existing_downstream: 0,
                    new_downstream: MAX_CREATE_NEURON_COUNT + 1,
                    fill_downstream_shortage: false,
                    siblings: 0,
                    upstream_depth: 0,
                    global_top_weight: 0,
                },
            })
            .await;
        assert!(result.is_err());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn select_candidates_under_creator_returns_seven() {
        let (manager, root) = test_manager();
        let creator = manager.ensure_creator().unwrap();
        let candidates = manager
            .select_candidates(CandidateQuery {
                n: DEFAULT_SELECT_N,
                source_id: Some(creator.id.clone()),
                min_new: 0,
            })
            .await
            .unwrap();
        assert_eq!(candidates.len(), 7);
        let downstream = manager
            .store()
            .unwrap()
            .list_direct_downstream(&creator.id, 10, &HashSet::new())
            .unwrap();
        assert_eq!(downstream.len(), 7);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ai_update_rejects_system_neuron() {
        let (manager, root) = test_manager();
        let creator = manager.ensure_creator().unwrap();
        let result = manager.update_content_for_ai(
            &creator.id,
            NeuronUpdate {
                desc: Some("changed".into()),
                content: None,
                ..Default::default()
            },
        );
        assert!(result.is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn select_one_from_falls_back_to_weight_without_selector() {
        let (manager, root) = test_manager();
        let low = insert_plain(&manager, "low", "low");
        let high = insert_plain(&manager, "high", "high");
        let low = manager.adjust_weight(&low.id, 1.0).unwrap();
        let high = manager.adjust_weight(&high.id, 9.0).unwrap();
        let selected = manager.select_one_from(&[low, high.clone()]).await.unwrap();
        assert_eq!(selected.id, high.id);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn create_neuron_ignores_model_weight_and_uses_zero() {
        let (manager, root) = test_manager();
        let source = insert_plain(&manager, "source", "source");
        let children = manager
            .create_neuron(
                CreateNeuronInput::Purpose("test purpose".into()),
                Some(&source.id),
                1,
            )
            .await
            .unwrap();
        assert_eq!(children.len(), 1);
        let child = &children[0];
        assert!((child.weight - 0.0).abs() < f64::EPSILON);
        let edge = manager
            .connections(&source.id)
            .unwrap()
            .into_iter()
            .find(|c| c.target == child.id)
            .unwrap();
        assert!((edge.weight - 0.0).abs() < f64::EPSILON);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn create_neuron_batch_returns_requested_count() {
        let (manager, root) = test_manager();
        let neurons = manager
            .create_neuron(
                CreateNeuronInput::Purpose("batch purpose".into()),
                None,
                3,
            )
            .await
            .unwrap();
        assert_eq!(neurons.len(), 3);
        assert!(neurons.iter().all(|n| n.system_type.is_none()));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn ensure_system_neuron_fills_own_downstream_not_creator() {
        let (manager, root) = test_manager();
        let creator = manager.ensure_creator().unwrap();
        let selector = manager
            .ensure_system_neuron(ASSISTANT_SELECT_NEURON, EnsureSystemOpts { reset: false })
            .await
            .unwrap();
        let selector_kids = manager
            .store()
            .unwrap()
            .list_direct_downstream(&selector.id, 20, &HashSet::new())
            .unwrap();
        assert_eq!(selector_kids.len(), DEFAULT_SELECT_N);
        let creator_kids = manager
            .store()
            .unwrap()
            .list_direct_downstream(&creator.id, 20, &HashSet::new())
            .unwrap();
        // Creator may gain kids when create_neuron picks a creation prompt; selector pool must be under selector.
        assert!(selector_kids.iter().all(|n| n.system_type.is_none()));
        assert!(!selector_kids.iter().any(|n| creator_kids.iter().any(|c| c.id == n.id)));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ensure_creator_uses_default_prompt_without_config() {
        let (manager, root) = test_manager();
        let creator = manager.ensure_creator().unwrap();
        assert_eq!(creator.system_type.as_deref(), Some(CREATOR_SYSTEM_TYPE));
        assert!(!creator.content.trim().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    // ── Creator self-iteration pool ─────────────────────────

    /// Build the creator plus its 7-variant pool.
    async fn seed_creator_pool(manager: &NeuronManager) -> String {
        let creator = manager.ensure_creator().unwrap();
        manager
            .select_candidates(CandidateQuery {
                n: DEFAULT_SELECT_N,
                source_id: Some(creator.id.clone()),
                min_new: 0,
            })
            .await
            .unwrap();
        creator.id
    }

    #[tokio::test]
    async fn create_neuron_attributes_lineage_and_usage() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        let before_len = {
            let store = manager.store().unwrap();
            store.get_variants(&creator_id, false).unwrap().len()
        };
        assert_eq!(before_len, DEFAULT_SELECT_N);

        let children = manager
            .create_neuron(CreateNeuronInput::Purpose("lineage test".into()), None, 1)
            .await
            .unwrap();
        assert_eq!(children.len(), 1);
        let parent_id = manager
            .store()
            .unwrap()
            .lineage_parent_id_of(&children[0].id)
            .unwrap()
            .expect("child must carry lineage");
        assert_ne!(parent_id, creator_id);

        let used = {
            let store = manager.store().unwrap();
            store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == parent_id)
                .expect("parent variant must exist")
        };
        assert_eq!(used.use_count, 1);
        assert!(used.last_used_at.is_some());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn create_neuron_filling_creator_links_lineage_to_creator() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        let children = manager
            .create_neuron(
                CreateNeuronInput::Purpose("fill test".into()),
                Some(&creator_id),
                1,
            )
            .await
            .unwrap();
        let parent_id = manager
            .store()
            .unwrap()
            .lineage_parent_id_of(&children[0].id)
            .unwrap()
            .expect("seed-born child must carry lineage");
        assert_eq!(parent_id, creator_id);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn observing_variant_promotes_after_use_and_rolls_back_on_regression() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        let (a, b) = {
            let store = manager.store().unwrap();
            let variants = store.get_variants(&creator_id, false).unwrap();
            let a = variants[0].neuron.id.clone();
            let b = variants[1].neuron.id.clone();
            store.set_variant_state(&a, Some("observing")).unwrap();
            store.set_variant_state(&b, Some("observing")).unwrap();
            (a, b)
        };

        // Idle observing slot stays put.
        manager.maybe_evolve_creator_variants().await.unwrap();
        let idle_state = {
            let store = manager.store().unwrap();
            store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == a)
                .unwrap()
                .variant_state
        };
        assert_eq!(idle_state.as_deref(), Some("observing"));

        // Regression (negative delta) without history: stays observing.
        manager.store().unwrap().accumulate_variant_delta(&a, -1.0).unwrap();
        manager.maybe_evolve_creator_variants().await.unwrap();
        let a_state = {
            let store = manager.store().unwrap();
            store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == a)
                .unwrap()
                .variant_state
        };
        assert_eq!(a_state.as_deref(), Some("observing"));

        // Clear the regression, then a used observing variant gets promoted.
        manager.store().unwrap().accumulate_variant_delta(&a, 1.0).unwrap();
        manager.store().unwrap().increment_variant_usage(&b).unwrap();
        manager.maybe_evolve_creator_variants().await.unwrap();
        let b_state = {
            let store = manager.store().unwrap();
            store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == b)
                .unwrap()
                .variant_state
        };
        assert_eq!(b_state.as_deref(), Some("active"));
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn active_variant_rewrites_at_threshold_and_moves_to_observing() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        let (target, original_content) = {
            let store = manager.store().unwrap();
            let variants = store.get_variants(&creator_id, false).unwrap();
            let target = variants[0].neuron.id.clone();
            let original_content = variants[0].neuron.content.clone();
            for _ in 0..3 {
                store.increment_variant_usage(&target).unwrap();
            }
            store.accumulate_variant_delta(&target, 2.0).unwrap();
            (target, original_content)
        };

        manager.maybe_evolve_creator_variants().await.unwrap();

        let (state, content) = {
            let store = manager.store().unwrap();
            let rewritten = store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == target)
                .unwrap();
            (rewritten.variant_state, rewritten.neuron.content)
        };
        assert_eq!(state.as_deref(), Some("observing"));
        assert_ne!(content, original_content);
        let version = manager
            .store()
            .unwrap()
            .latest_version_of(&target)
            .unwrap()
            .expect("evolve must archive a version");
        assert_eq!(version.source, "evolve");
        assert_eq!(version.content, original_content);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn manual_edited_variant_is_locked_from_rewrite_and_elimination() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        let (target, original_content) = {
            let store = manager.store().unwrap();
            let variants = store.get_variants(&creator_id, false).unwrap();
            let target = variants[0].neuron.id.clone();
            let original_content = variants[0].neuron.content.clone();
            for _ in 0..3 {
                store.increment_variant_usage(&target).unwrap();
            }
            store.accumulate_variant_delta(&target, 2.0).unwrap();
            store.set_manual_edited(&target, true).unwrap();
            (target, original_content)
        };

        manager.maybe_evolve_creator_variants().await.unwrap();

        let (state, content, has_version) = {
            let store = manager.store().unwrap();
            let locked = store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == target)
                .unwrap();
            (
                locked.variant_state,
                locked.neuron.content,
                store.latest_version_of(&target).unwrap().is_some(),
            )
        };
        // Seed variants carry NULL state, which means active (not observing).
        assert_ne!(state.as_deref(), Some("observing"));
        assert_eq!(content, original_content);
        assert!(!has_version);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn eliminated_variant_rolls_back_to_archived_version() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        let target = {
            let store = manager.store().unwrap();
            let variants = store.get_variants(&creator_id, false).unwrap();
            let target = variants[0].neuron.id.clone();
            store
                .insert_neuron_version(&target, "archived-v1", "seed", None)
                .unwrap();
            store
                .update_neuron(
                    &target,
                    NeuronUpdate {
                        desc: None,
                        content: Some("current-v2".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            for _ in 0..3 {
                store.accumulate_variant_delta(&target, -1.0).unwrap();
            }
            target
        };

        manager.maybe_evolve_creator_variants().await.unwrap();

        let rolled_content = {
            let store = manager.store().unwrap();
            store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == target)
                .unwrap()
                .neuron
                .content
        };
        assert_eq!(rolled_content, "archived-v1");
        let version = manager
            .store()
            .unwrap()
            .latest_version_of(&target)
            .unwrap()
            .expect("rollback must be recorded");
        assert_eq!(version.source, "rollback");
        assert_eq!(version.content, "archived-v1");
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn legacy_null_variant_state_is_treated_as_active() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        {
            let store = manager.store().unwrap();
            assert_eq!(
                store.get_variants(&creator_id, true).unwrap().len(),
                DEFAULT_SELECT_N
            );
        }

        manager.maybe_evolve_creator_variants().await.unwrap();

        let after = {
            let store = manager.store().unwrap();
            store.get_variants(&creator_id, false).unwrap()
        };
        assert_eq!(after.len(), DEFAULT_SELECT_N);
        assert!(after.iter().all(|v| v.variant_state.is_none()));
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn admin_update_marks_variant_manual_edited() {
        let (manager, root) = test_manager();
        let creator = manager.ensure_creator().unwrap();
        let variant = manager
            .create_plain(
                NeuronCreate {
                    desc: "v".into(),
                    content: "original".into(),
                    ..Default::default()
                },
                Some(&creator.id),
            )
            .unwrap();
        manager
            .update_content_for_admin(
                &variant.id,
                NeuronUpdate {
                    desc: None,
                    content: Some("edited".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let edited = {
            let store = manager.store().unwrap();
            store
                .get_variants(&creator.id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == variant.id)
                .unwrap()
        };
        assert!(edited.manual_edited);
        assert_eq!(edited.neuron.content, "edited");
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    // ── Capacity & low-value recycling ─────────────────────────

    fn write_config_with_capacity(root: &std::path::Path, capacity: usize) {
        fs::write(
            root.join("config.json"),
            format!(
                r#"{{"neurons":{{"bootstrap":{{"create_neuron_prompt":"create a neuron"}}}},"neuron":{{"capacity":{capacity}}}}}"#
            ),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn recycle_if_over_capacity_recycles_lowest_value_and_exempts_system() {
        let (manager, root) = test_manager();
        write_config_with_capacity(&root, 3);

        for index in 0..5 {
            insert_plain(&manager, &format!("n{index}"), "plain");
        }
        manager
            .store()
            .unwrap()
            .create_neuron(NeuronCreate {
                desc: "sys".into(),
                content: "prompt".into(),
                system_type: Some("sys_type".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(manager.list().unwrap().len(), 6);

        let recycled = manager.recycle_if_over_capacity().unwrap();
        assert_eq!(recycled, 3);
        // 2 个普通 + 1 个系统保留，恰好等于容量 3。
        assert_eq!(manager.list().unwrap().len(), 3);
        assert!(manager
            .get_neuron_by_system_type("sys_type")
            .unwrap()
            .is_some());

        // 幂等：未超容量返回 0。
        assert_eq!(manager.recycle_if_over_capacity().unwrap(), 0);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn recycle_spares_used_neurons() {
        let (manager, root) = test_manager();
        write_config_with_capacity(&root, 2);

        let used = insert_plain(&manager, "used", "used");
        insert_plain(&manager, "low-a", "a");
        insert_plain(&manager, "low-b", "b");
        manager.store().unwrap().mark_used(&used.id).unwrap();

        let recycled = manager.recycle_if_over_capacity().unwrap();
        assert_eq!(recycled, 1);
        let remaining: Vec<String> = manager
            .list()
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert!(remaining.contains(&used.id), "used neuron must survive");
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn select_one_marks_usage_as_active_signal() {
        let (manager, root) = test_manager();
        let n = insert_plain(&manager, "target", "target");
        let selected = manager.select_one_from(&[n.clone()]).await.unwrap();
        assert_eq!(selected.id, n.id);
        let stored = manager.get(&n.id).unwrap().unwrap();
        assert_eq!(stored.use_count, 1);
        assert!(stored.last_used_at.is_some());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }
}

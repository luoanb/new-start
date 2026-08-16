//! 候选池与选型领域服务：候选填充、LLM 选型、权重回退、助手邻域候选。
//!
//! 自含生成原语（ensure_creator / generate_drafts / persist_plain /
//! create_neuron_user_prompt / fill_candidates_batch），因此依赖方向固定为
//! `Creation → Selection → {store, model_caller, config, tool_registry}`，无环。
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, RwLock},
};

use crate::core::{
    error::{AppError, AppResult},
    insert_catalog::InsertCatalog,
    model_call_input::{ModelAppendTemplate, ModelCallInput},
    models::{
        AssistantCandidateScope, CandidateQuery, CreateNeuronInput, GeneratedNeuronDraft,
        ModelMessage, NeighborhoodPoolPolicy, Neuron, NeuronCreate,
    },
    neuron::{
        config::NeuronConfigReader,
        model::{extract_json_object, parse_generated_drafts, NeuronModelCaller},
        store::NeuronStore,
    },
    tool_registry::ToolRegistry,
};

use super::{
    lock_error,
    manager::{
        ASSISTANT_SELECT_NEURON, CREATOR_SYSTEM_TYPE, DEFAULT_SELECT_N, MAX_CREATE_NEURON_COUNT,
    },
};

pub(crate) struct NeuronSelection {
    store: Arc<Mutex<NeuronStore>>,
    model_caller: Arc<dyn NeuronModelCaller>,
    config: NeuronConfigReader,
    /// creator 神经元 id 缓存（create_neuron 种子根）。删除时由调用方失效。
    creator_id: Mutex<Option<String>>,
    /// 共享工具注册表（与 Gateway 同一 `Arc<RwLock>`）：读锁 clone 后立即释放，不跨 await。
    tool_registry: Arc<RwLock<ToolRegistry>>,
}

impl NeuronSelection {
    pub(crate) fn new(
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

    pub(crate) fn store(&self) -> AppResult<std::sync::MutexGuard<'_, NeuronStore>> {
        self.store.lock().map_err(lock_error)
    }

    /// 惰性加载（或种子创建）creator 系统神经元；id 缓存命中直接返回。
    pub(crate) fn ensure_creator(&self) -> AppResult<Neuron> {
        let mut cached_id = self.creator_id.lock().map_err(lock_error)?;
        if let Some(id) = cached_id.clone() {
            if let Some(neuron) = self.store()?.get_neuron(&id)? {
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

        if let Some(neuron) = self
            .store()?
            .get_neuron_by_system_type(CREATOR_SYSTEM_TYPE)?
        {
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

    /// 管理面删除神经元后调用：若删除的是 creator 则失效缓存（行为与拆分前一致）。
    pub(crate) fn clear_creator_cache_if_matches(&self, id: &str) {
        if let Ok(mut creator_id) = self.creator_id.lock() {
            if creator_id.as_deref() == Some(id) {
                *creator_id = None;
            }
        }
    }

    /// Build an "available tools" block for creator prompts so `tool_ids` can only
    /// be picked from tools that actually exist in the registry (no invented names).
    pub(crate) fn available_tools_block(&self) -> String {
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

    pub(crate) async fn select_candidates(
        &self,
        query: CandidateQuery,
    ) -> AppResult<Vec<Neuron>> {
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

    pub(crate) async fn select_one(&self, query: CandidateQuery) -> AppResult<Neuron> {
        self.select_one_with_history(query, &[]).await
    }

    /// Select one neuron; `history` is read-only conversation context (not persisted by this call).
    pub(crate) async fn select_one_with_history(
        &self,
        query: CandidateQuery,
        history: &[ModelMessage],
    ) -> AppResult<Neuron> {
        let mut query = query;
        if query.n == 0 {
            query.n = DEFAULT_SELECT_N;
        }
        // 回挂边锚点：候选池锚点 = query.source_id（有源时命中即为其直接下游，通常空操作；幂等安全）。
        let link_source = query.source_id.clone();
        let candidates = self.select_candidates(query).await?;
        self.select_one_from_with_history(&candidates, history, link_source.as_deref())
            .await
    }

    /// Build Assistant candidates without invoking the selection model.
    pub(crate) async fn select_assistant_candidates(
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
            .store()?
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
            let siblings =
                self.store()?
                    .list_direct_downstream(&parent.id, policy.siblings, &selected_ids)?;
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

    pub(crate) async fn select_one_from(&self, candidates: &[Neuron]) -> AppResult<Neuron> {
        // 调用方无锚点信息，不建回挂边。
        self.select_one_from_with_history(candidates, &[], None).await
    }

    pub(crate) async fn select_one_from_with_history(
        &self,
        candidates: &[Neuron],
        history: &[ModelMessage],
        link_source: Option<&str>,
    ) -> AppResult<Neuron> {
        if candidates.is_empty() {
            return Err(AppError::InvalidInput(
                "No neuron candidates available for selection".into(),
            ));
        }
        match self.try_llm_select(candidates, history).await {
            Ok(neuron) => {
                // 模选命中后回挂边：source → target（权重 0，幂等）。失败不阻塞选型（与 mark_used 同策略）。
                if let Err(error) = self.maybe_link_to_source(link_source, &neuron) {
                    tracing::warn!(
                        phase = "select_one.link_back",
                        error = %error,
                        "link back skipped due to error"
                    );
                }
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

    /// 模选回挂边规则：source 有值且 target 不是 source 直接下游时新建 `source → target` 边。
    /// - source 为 None → 跳过（无锚点，不建边）；
    /// - `source == target.id` → 跳过（不自环）；
    /// - `connection_exists` 为真 → 跳过（幂等）；
    /// - 否则 `link(source, target.id, 0.0)`（新边恒权重 0）。
    fn maybe_link_to_source(&self, source: Option<&str>, target: &Neuron) -> AppResult<()> {
        let Some(source) = source else {
            return Ok(());
        };
        if source == target.id {
            return Ok(());
        }
        if self.store()?.connection_exists(source, &target.id)? {
            return Ok(());
        }
        self.store()?.link(source, &target.id, 0.0)?;
        tracing::info!(
            phase = "select_one.link_back",
            source,
            target_id = %target.id,
            "linked back source -> target after model selection"
        );
        Ok(())
    }

    fn resolve_source_id(&self, source_id: Option<&str>) -> AppResult<Option<String>> {
        let Some(source_id) = source_id else {
            return Ok(None);
        };
        if self.store()?.get_neuron(source_id)?.is_none() {
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
            .store()?
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
        let wire = ModelCallInput::assemble(
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
        let output = self.model_caller.call_model(wire.messages).await?;
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
        let user_prompt =
            self.create_neuron_user_prompt(&CreateNeuronInput::Purpose(purpose), count, source_id)?;
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

    pub(crate) async fn ensure_own_candidate_pool(&self, root_id: &str) -> AppResult<()> {
        let _ = self
            .select_candidates(CandidateQuery {
                n: DEFAULT_SELECT_N,
                source_id: Some(root_id.to_string()),
                min_new: 0,
            })
            .await?;
        Ok(())
    }

    pub(crate) async fn generate_draft(
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

    pub(crate) async fn generate_drafts(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        expected: usize,
        history: &[ModelMessage],
    ) -> AppResult<Vec<GeneratedNeuronDraft>> {
        // Append subject is the draft manual; creator/prompt neuron stays in role_system.
        let insert = InsertCatalog::require("neuron.draft_from_model");
        let wire = ModelCallInput::assemble(
            history,
            system_prompt,
            insert,
            user_prompt,
            ModelAppendTemplate::Manual,
        );
        tracing::info!(
            phase = "generate_drafts",
            message_count = wire.messages.len(),
            user_len = user_prompt.len(),
            expected,
            "generate_drafts model call start"
        );
        let output = match self.model_caller.call_model(wire.messages).await {
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

    pub(crate) fn create_neuron_user_prompt(
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

    /// 前端手动创建 / 创建流程持久化：store 直持久化，不触发 LLM 草稿生成。
    /// link_to = None => 孤立神经元；Some(id) => 该神经元的下游神经元（自动建边，边权重 0）。
    pub(crate) fn persist_plain(
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

    pub(crate) fn persist_system_root(&self, mut create: NeuronCreate) -> AppResult<Neuron> {
        if create
            .system_type
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            return Err(AppError::InvalidInput(
                "persist_system_root requires system_type".into(),
            ));
        }
        create.weight = 0.0;
        self.store()?.create_neuron(create)
    }
}

/// 权重回退选型：从最高权重并列集合中按时间种子均匀选一（确定性折中）。
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

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

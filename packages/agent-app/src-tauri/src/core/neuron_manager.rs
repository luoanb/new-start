use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use serde_json::Value;

use super::{
    error::{AppError, AppResult},
    models::{
        BootstrapReport, CandidateQuery, Connection, CreateNeuronInput, EnsureSystemOpts,
        GeneratedNeuronDraft, Neuron, NeuronCreate, NeuronSubgraph, NeuronUpdate,
        SystemPromptStatus,
    },
    neuron_config::NeuronConfigReader,
    neuron_model::NeuronModelCaller,
    insert_catalog::InsertCatalog,
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
const DEFAULT_SELECT_N: usize = 7;
const MAX_CREATE_NEURON_COUNT: usize = 10;

pub struct NeuronManager {
    store: Arc<Mutex<NeuronStore>>,
    model_caller: Arc<dyn NeuronModelCaller>,
    config: NeuronConfigReader,
    creator_id: Mutex<Option<String>>,
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
    ) -> Self {
        Self {
            store,
            model_caller,
            config,
            creator_id: Mutex::new(None),
        }
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
        self.store()?.update_neuron(id, update)
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
            "select_candidates ok"
        );
        Ok(selected)
    }

    pub async fn select_one(&self, query: CandidateQuery) -> AppResult<Neuron> {
        let mut query = query;
        if query.n == 0 {
            query.n = DEFAULT_SELECT_N;
        }
        let candidates = self.select_candidates(query).await?;
        self.select_one_from(&candidates).await
    }

    pub async fn select_one_from(&self, candidates: &[Neuron]) -> AppResult<Neuron> {
        if candidates.is_empty() {
            return Err(AppError::InvalidInput(
                "No neuron candidates available for selection".into(),
            ));
        }
        match self.try_llm_select(candidates).await {
            Ok(neuron) => {
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
                pick_by_weight(candidates)
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
        let prompt_content = if filling_creator {
            creator.content.clone()
        } else {
            self.select_one(CandidateQuery {
                n: DEFAULT_SELECT_N,
                source_id: Some(creator.id.clone()),
                min_new: 0,
            })
            .await?
            .content
        };
        let user_prompt = self.create_neuron_user_prompt(&input, count, link_to)?;
        let drafts = self
            .generate_drafts(&prompt_content, &user_prompt, count)
            .await?;
        let mut created = Vec::with_capacity(drafts.len());
        for draft in drafts {
            let create = NeuronCreate {
                desc: draft.desc,
                content: draft.content,
                weight: 0.0,
                system_type: None,
                tool_ids: draft.tool_ids,
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
        let user_prompt = format!(
            "Write a system prompt neuron with system_type={system_type}.\n\
             Requirements:\n\
             - `content` must be a full executable system prompt: role, decision criteria, steps, output contract, hard constraints.\n\
             - Prefer 200–800 Chinese characters (or equivalent); no slogans or placeholders.\n\
             - One responsibility aligned with system_type={system_type}.\n\
             - Do not assign importance scores; system forces initial weight to 0.\n\
             - `tool_ids`: only truly needed tools; else []. Do not invent tool names.\n\
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
        })?;
        *cached_id = Some(neuron.id.clone());
        tracing::info!(
            phase = "ensure_creator",
            neuron_id = %neuron.id,
            "creator created from seed"
        );
        Ok(neuron)
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

    async fn try_llm_select(&self, candidates: &[Neuron]) -> AppResult<Neuron> {
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
        let system = InsertCatalog::system_with_insert(&selector.content, "neuron.select_one");
        let output = self
            .model_caller
            .call_model(&system, &payload.to_string())
            .await?;
        let decision = extract_json_object(&output)?;
        let neuron_id = decision
            .get("neuron_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::InvalidInput("select neuron response missing neuron_id".into())
            })?;
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
        let mut drafts = self.generate_drafts(system_prompt, user_prompt, 1).await?;
        drafts.pop().ok_or_else(|| {
            AppError::NeuronBootstrapFailed("Generated neuron list was empty".into())
        })
    }

    async fn generate_drafts(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        expected: usize,
    ) -> AppResult<Vec<GeneratedNeuronDraft>> {
        let system = InsertCatalog::system_with_insert(system_prompt, "neuron.draft_from_model");
        tracing::info!(
            phase = "generate_drafts",
            system_len = system.len(),
            user_len = user_prompt.len(),
            expected,
            "generate_drafts model call start"
        );
        let output = match self.model_caller.call_model(&system, user_prompt).await {
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
            .generate_drafts(&prompt_content, &user_prompt, count)
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
        Ok(match input {
            CreateNeuronInput::Purpose(purpose) => format!(
                "Create {count_word} single-responsibility neuron(s) for the purpose below.{link_note}\n\
                 Requirements:\n\
                 - Each neuron focuses on one job only; do not bundle unrelated skills.\n\
                 - `content` must be an executable prompt/knowledge block (role, when to use / not use, steps, output format, hard constraints).\n\
                 - Prefer 200–800 Chinese characters (or equivalent) in `content`; no slogans or placeholders.\n\
                 - Do not assign importance scores; system forces initial weight to 0.\n\
                 - `tool_ids`: only truly needed tools; else []. Do not invent tool names.\n\
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
                 - `tool_ids`: only truly needed tools; else []. Do not invent tool names.\n\
                 - {list_contract}\n\
                 Context: {}",
                serde_json::to_string(messages).unwrap_or_default()
            ),
        })
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

    struct MockModelCaller {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl NeuronModelCaller for MockModelCaller {
        async fn call_model(&self, _system_prompt: &str, user_prompt: &str) -> AppResult<String> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
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
}

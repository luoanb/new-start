//! 创建与启动编排领域服务：统一创建流程、系统神经元、会话规格、bootstrap。
//!
//! 依赖方向：`Creation → {Query, Selection, specs}`。生成原语（草稿、持久化、
//! creator 种子）统一复用 `NeuronSelection`，避免与选型域双向耦合。
use std::sync::{Arc, Mutex};

use crate::core::{
    error::{AppError, AppResult},
    models::{
        BootstrapReport, CandidateQuery, CreateNeuronInput, EnsureSystemOpts, Neuron,
        NeuronCreate, SessionBehavior, SystemPromptStatus,
    },
    neuron::{
        config::NeuronConfigReader,
        query::NeuronQuery,
        selection::NeuronSelection,
        spec::SessionSpecManager,
        store::NeuronStore,
    },
};

use super::{
    lock_error,
    manager::{
        ASSISTANT_SELECT_NEURON, DEFAULT_SELECT_N, MAX_CREATE_NEURON_COUNT, REBOOTSTRAP_SYSTEM_TYPES,
        SESSION_ASSISTANT_DIALOGUE, default_assistant_dialogue_behavior,
        default_behavior_for_system_type,
    },
};

pub(crate) struct NeuronCreation {
    store: Arc<Mutex<NeuronStore>>,
    config: NeuronConfigReader,
    /// 会话规格管理子组件（behavior 只读/写路径统一收敛于此）。
    specs: SessionSpecManager,
    query: Arc<NeuronQuery>,
    selection: Arc<NeuronSelection>,
}

impl NeuronCreation {
    pub(crate) fn new(
        store: Arc<Mutex<NeuronStore>>,
        config: NeuronConfigReader,
        query: Arc<NeuronQuery>,
        selection: Arc<NeuronSelection>,
    ) -> Self {
        Self {
            specs: SessionSpecManager::new(Arc::clone(&store)),
            store,
            config,
            query,
            selection,
        }
    }

    fn store(&self) -> AppResult<std::sync::MutexGuard<'_, NeuronStore>> {
        self.store.lock().map_err(lock_error)
    }

    /// Ordinary neuron(s) via unified creation flow (pool→7→1 under creator).
    /// `count` must be in `1..=10`. Model returns a JSON list of drafts; all are persisted.
    pub(crate) async fn create_neuron(
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

        let creator = self.selection.ensure_creator()?;
        let filling_creator = link_to == Some(creator.id.as_str());
        let (prompt_content, lineage_parent_id) = if filling_creator {
            // Seed-born: lineage points at the creator itself.
            (creator.content.clone(), Some(creator.id.clone()))
        } else {
            let variant = self
                .selection
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
        let user_prompt = self
            .selection
            .create_neuron_user_prompt(&input, count, link_to)?;
        let drafts = self
            .selection
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
            created.push(self.selection.persist_plain(create, link_to)?);
        }
        Ok(created)
    }

    /// Ensure a system prompt root (any system_type). Idempotent unless `opts.reset`.
    pub(crate) async fn ensure_system_neuron(
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
            if let Some(existing) = self.query.get_by_system_type(system_type)? {
                let _ = self.store()?.unlink_all_edges_of(&existing.id)?;
                let _ = self.query.delete_for_admin(&existing.id)?;
                self.selection.clear_creator_cache_if_matches(&existing.id);
                tracing::info!(
                    phase = "ensure_system_neuron",
                    system_type,
                    neuron_id = %existing.id,
                    "reset deleted existing system neuron"
                );
            }
        } else if let Some(existing) = self.query.get_by_system_type(system_type)? {
            tracing::info!(
                phase = "ensure_system_neuron",
                system_type,
                neuron_id = %existing.id,
                "ensure_system_neuron hit existing; filling own downstream pool"
            );
            self.selection.ensure_own_candidate_pool(&existing.id).await?;
            return Ok(existing);
        }

        let creator = self.selection.ensure_creator()?;
        let tools_note = self.selection.available_tools_block();
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
        let draft = match self.selection.generate_draft(&creator.content, &user_prompt).await {
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
        let created = self.selection.persist_system_root(NeuronCreate {
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
        // 裁决类系统神经元创建即注册默认 behavior（Fixed + 各自 insert_id），
        // 使业务侧裁决调用无需额外映射即可按 Fixed 语义取提示词。
        if let Some(default_behavior) = default_behavior_for_system_type(system_type) {
            self.store()?
                .set_behavior(&created.id, Some(&default_behavior))?;
        }
        tracing::info!(
            phase = "ensure_system_neuron",
            system_type,
            neuron_id = %created.id,
            "ensure_system_neuron created; filling own downstream pool"
        );
        self.selection.ensure_own_candidate_pool(&created.id).await?;
        Ok(created)
    }

    /// 懒创建规格神经元（复用 ensure_system_neuron 骨架：存在复用 / reset 重建）。
    /// 新建时经 `specs` 写 behavior 与 content；已存在不覆盖。
    pub(crate) async fn ensure_session_neuron(
        &self,
        system_type: &str,
        behavior: SessionBehavior,
        content: Option<String>,
        opts: EnsureSystemOpts,
    ) -> AppResult<Neuron> {
        let system_type = system_type.trim();
        if system_type.is_empty() || !system_type.starts_with("session.") {
            return Err(AppError::InvalidInput(format!(
                "session spec system_type must start with 'session.': {system_type}"
            )));
        }
        if opts.reset {
            if let Some(existing) = self.query.get_by_system_type(system_type)? {
                let _ = self.store()?.unlink_all_edges_of(&existing.id)?;
                let _ = self.query.delete_for_admin(&existing.id)?;
                self.selection.clear_creator_cache_if_matches(&existing.id);
                tracing::info!(
                    phase = "ensure_session_neuron",
                    system_type,
                    neuron_id = %existing.id,
                    "reset deleted existing session spec"
                );
            }
        }
        self.specs
            .ensure_session_neuron(system_type, &behavior, content)
    }

    /// 只读：取会话规格的 behavior（校验 system_type + behavior 非空）。
    pub(crate) fn get_session_behavior(&self, neuron_id: &str) -> AppResult<SessionBehavior> {
        self.specs.get_session_behavior(neuron_id)
    }

    /// 管理面更新入口：只写 behavior（specs 子组件），不触碰 content。
    pub(crate) fn update_behavior_for_admin(
        &self,
        id: &str,
        behavior: SessionBehavior,
    ) -> AppResult<Neuron> {
        self.specs.update_behavior_for_admin(id, behavior)
    }

    /// 列出所有 `session.%` 规格神经元（含 behavior 摘要）。
    pub(crate) fn list_session_specs(&self) -> AppResult<Vec<SystemPromptStatus>> {
        self.specs.list_specs()
    }

    /// Startup readiness: creator + selector only.
    pub(crate) async fn bootstrap(&self) -> AppResult<BootstrapReport> {
        tracing::info!(phase = "bootstrap", "bootstrap start");
        let creator = self.selection.ensure_creator()?;
        // First-boot: ensure the creator owns its candidate pool (7 active slots).
        self.selection.ensure_own_candidate_pool(&creator.id).await?;
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
        // 内建会话规格懒注册（失败不阻断启动；可通过管理面手动补建）。
        let session_defaults = self.config.session_defaults()?;
        match self
            .ensure_session_neuron(
                SESSION_ASSISTANT_DIALOGUE,
                default_assistant_dialogue_behavior(Some(&session_defaults)),
                None,
                EnsureSystemOpts { reset: false },
            )
            .await
        {
            Ok(neuron) => {
                tracing::info!(
                    phase = "bootstrap",
                    session_spec = SESSION_ASSISTANT_DIALOGUE,
                    neuron_id = %neuron.id,
                    "session spec ensured"
                );
            }
            Err(error) => {
                tracing::warn!(
                    phase = "bootstrap",
                    session_spec = SESSION_ASSISTANT_DIALOGUE,
                    error_code = error.code(),
                    error = %error,
                    "failed to ensure session spec; continuing bootstrap"
                );
            }
        }
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
    pub(crate) async fn rebootstrap(&self) -> AppResult<BootstrapReport> {
        tracing::info!(phase = "rebootstrap", "rebootstrap start");
        let _ = self.selection.ensure_creator()?;
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

    /// 前端手动创建：store 直持久化，不触发 LLM 草稿生成。
    /// link_to = None => 孤立神经元；Some(id) => 该神经元的下游神经元（自动建边，边权重 0）。
    pub(crate) fn create_plain(
        &self,
        create: NeuronCreate,
        link_to: Option<&str>,
    ) -> AppResult<Neuron> {
        self.selection.persist_plain(create, link_to)
    }
}

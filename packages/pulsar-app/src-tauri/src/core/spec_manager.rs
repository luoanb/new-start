use std::sync::{Arc, Mutex};

use super::{
    error::{AppError, AppResult},
    models::{Neuron, NeuronCreate, SessionBehavior, SystemPromptStatus},
    neuron_store::NeuronStore,
};

/// Session spec management (管理面): 会话规格 = `system_type = 'session.<id>'` 的系统神经元 +
/// 挂载在 `behavior` 列的 [`SessionBehavior`]。
///
/// 只读与写路径统一收敛于此：
/// - 只读：`get_session_behavior` / `list_specs`
/// - 写：`ensure_session_neuron`（懒创建）/ `update_behavior_for_admin`
pub struct SessionSpecManager {
    store: Arc<Mutex<NeuronStore>>,
}

impl std::fmt::Debug for SessionSpecManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionSpecManager").finish_non_exhaustive()
    }
}

impl SessionSpecManager {
    pub fn new(store: Arc<Mutex<NeuronStore>>) -> Self {
        Self { store }
    }

    fn store(&self) -> AppResult<std::sync::MutexGuard<'_, NeuronStore>> {
        self.store
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock neuron store: {}", e)))
    }

    /// 校验 system_type 必须非空且以 `session.` 开头。
    fn validate_system_type(system_type: &str) -> AppResult<()> {
        let system_type = system_type.trim();
        if system_type.is_empty() {
            return Err(AppError::InvalidInput(
                "session spec system_type cannot be empty".into(),
            ));
        }
        if !system_type.starts_with("session.") {
            return Err(AppError::InvalidInput(format!(
                "session spec system_type must start with 'session.': {system_type}"
            )));
        }
        Ok(())
    }

    /// 懒创建规格神经元：content 取传入（`None` 时为空占位），新建时写 behavior；已存在则原样返回（不覆盖）。
    pub fn ensure_session_neuron(
        &self,
        system_type: &str,
        behavior: &SessionBehavior,
        content: Option<String>,
    ) -> AppResult<Neuron> {
        let system_type = system_type.trim();
        Self::validate_system_type(system_type)?;
        if let Some(existing) = self.store()?.get_neuron_by_system_type(system_type)? {
            return Ok(existing);
        }
        let created = self.store()?.create_neuron(NeuronCreate {
            desc: system_type.to_string(),
            content: content.unwrap_or_default(),
            weight: 0.0,
            system_type: Some(system_type.to_string()),
            tool_ids: vec![],
            lineage_parent_id: None,
            variant_state: None,
        })?;
        self.store()?.set_behavior(&created.id, Some(behavior))
    }

    /// 校验神经元是有效会话规格并取回 behavior。
    pub fn get_session_behavior(&self, neuron_id: &str) -> AppResult<SessionBehavior> {
        let neuron = self
            .store()?
            .get_neuron(neuron_id)?
            .ok_or_else(|| AppError::NeuronNotFound(neuron_id.to_string()))?;
        match neuron.system_type.as_deref() {
            Some(system_type) => Self::validate_system_type(system_type)?,
            None => {
                return Err(AppError::InvalidInput(format!(
                    "neuron {neuron_id} is not a session spec (no system_type)"
                )))
            }
        }
        neuron.behavior.ok_or_else(|| {
            AppError::InvalidInput(format!(
                "neuron {neuron_id} is a session spec but has no behavior"
            ))
        })
    }

    /// 管理面更新入口：只写 behavior，不触碰 content（避免与 update_content_for_admin 双写）。
    pub fn update_behavior_for_admin(
        &self,
        id: &str,
        behavior: SessionBehavior,
    ) -> AppResult<Neuron> {
        // 先校验目标确实是会话规格神经元，再写入。
        self.get_session_behavior(id)?;
        self.store()?.set_behavior(id, Some(&behavior))
    }

    /// 列出所有 `system_type LIKE 'session.%'` 的规格神经元（含 behavior 摘要，供前端「管理好后发起会话」）。
    pub fn list_specs(&self) -> AppResult<Vec<SystemPromptStatus>> {
        let neurons = self.store()?.list_neurons()?;
        let mut specs = Vec::new();
        for neuron in neurons {
            let Some(system_type) = neuron.system_type.as_deref() else {
                continue;
            };
            if !system_type.starts_with("session.") {
                continue;
            }
            specs.push(SystemPromptStatus {
                system_type: system_type.to_string(),
                neuron_id: Some(neuron.id),
                behavior: neuron.behavior,
            });
        }
        specs.sort_by(|a, b| a.system_type.cmp(&b.system_type));
        Ok(specs)
    }
}

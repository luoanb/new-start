//! 知识查询与治理（管理面）领域服务：纯 store 读/写转发。
//!
//! 不持有模型调用能力，只做数据访问层的薄封装：
//! 单点查询、列表、网络遍历、内容更新、权重调整、管理面图操作、
//! 分页/搜索/类型筛选、使用信号、容量回收。
use std::sync::{Arc, Mutex};

use crate::core::{
    error::{AppError, AppResult},
    log_phase::PHASE_NEURON_RECYCLE,
    models::{
        Connection, Neuron, NeuronKindFilter, NeuronPage, NeuronSubgraph, NeuronUpdate,
        SystemPromptStatus,
    },
    neuron::{
        config::NeuronConfigReader,
        store::NeuronStore,
    },
};

use super::lock_error;

pub(crate) struct NeuronQuery {
    store: Arc<Mutex<NeuronStore>>,
    config: NeuronConfigReader,
}

impl NeuronQuery {
    pub(crate) fn new(store: Arc<Mutex<NeuronStore>>, config: NeuronConfigReader) -> Self {
        Self { store, config }
    }

    pub(crate) fn store(&self) -> AppResult<std::sync::MutexGuard<'_, NeuronStore>> {
        self.store.lock().map_err(lock_error)
    }

    pub(crate) fn get(&self, id: &str) -> AppResult<Option<Neuron>> {
        self.store()?.get_neuron(id)
    }

    /// IPC-stable alias for [`Self::get`].
    pub(crate) fn get_neuron(&self, id: &str) -> AppResult<Option<Neuron>> {
        self.get(id)
    }

    pub(crate) fn get_by_system_type(&self, system_type: &str) -> AppResult<Option<Neuron>> {
        self.store()?.get_neuron_by_system_type(system_type)
    }

    /// IPC-stable alias for [`Self::get_by_system_type`].
    pub(crate) fn get_neuron_by_system_type(&self, system_type: &str) -> AppResult<Option<Neuron>> {
        self.get_by_system_type(system_type)
    }

    pub(crate) fn list(&self) -> AppResult<Vec<Neuron>> {
        self.store()?.list_neurons()
    }

    /// IPC-stable alias for [`Self::list`].
    pub(crate) fn list_neurons(&self) -> AppResult<Vec<Neuron>> {
        self.list()
    }

    pub(crate) fn connections(&self, id: &str) -> AppResult<Vec<Connection>> {
        self.store()?.get_connections(id)
    }

    /// IPC-stable alias for [`Self::connections`].
    pub(crate) fn get_connections(&self, id: &str) -> AppResult<Vec<Connection>> {
        self.connections(id)
    }

    pub(crate) fn network(&self, id: &str, max_depth: usize) -> AppResult<NeuronSubgraph> {
        self.store()?.get_network(id, max_depth)
    }

    /// IPC-stable alias for [`Self::network`].
    pub(crate) fn get_network(&self, id: &str, max_depth: usize) -> AppResult<NeuronSubgraph> {
        self.network(id, max_depth)
    }

    pub(crate) fn update_content_for_ai(
        &self,
        id: &str,
        update: NeuronUpdate,
    ) -> AppResult<Neuron> {
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

    pub(crate) fn update_content_for_admin(
        &self,
        id: &str,
        update: NeuronUpdate,
    ) -> AppResult<Neuron> {
        let store = self.store()?;
        let updated = store.update_neuron(id, update)?;
        // Manual edits lock the neuron out of auto-rewrite / elimination.
        store.set_manual_edited(id, true)?;
        Ok(updated)
    }

    pub(crate) fn adjust_weight(&self, id: &str, delta: f64) -> AppResult<Neuron> {
        self.store()?.adjust_weight(id, delta)
    }

    pub(crate) fn adjust_edge_weight(
        &self,
        source: &str,
        target: &str,
        delta: f64,
    ) -> AppResult<Connection> {
        self.store()?
            .adjust_connection_weight(source, target, delta)
    }

    pub(crate) fn list_system_prompt_status(
        &self,
        types: &[&str],
    ) -> AppResult<Vec<SystemPromptStatus>> {
        let mut out = Vec::with_capacity(types.len());
        for system_type in types {
            let neuron_id = self
                .get_by_system_type(system_type)?
                .map(|neuron| neuron.id);
            out.push(SystemPromptStatus {
                system_type: (*system_type).to_string(),
                neuron_id,
                behavior: None,
            });
        }
        Ok(out)
    }

    /// Admin graph op：仅删除数据行。creator 缓存失效由调用方（Facade / Creation）
    /// 组合调用 `NeuronSelection::clear_creator_cache_if_matches` 完成。
    pub(crate) fn delete_for_admin(&self, id: &str) -> AppResult<bool> {
        self.store()?.delete_neuron(id)
    }

    pub(crate) fn link_for_admin(
        &self,
        source: &str,
        target: &str,
        weight: f64,
    ) -> AppResult<Connection> {
        self.store()?.link(source, target, weight)
    }

    pub(crate) fn unlink_for_admin(&self, source: &str, target: &str) -> AppResult<bool> {
        self.store()?.unlink(source, target)
    }

    pub(crate) fn set_tool_ids_for_admin(
        &self,
        id: &str,
        tool_ids: Vec<String>,
    ) -> AppResult<Neuron> {
        self.store()?.set_tool_ids(id, tool_ids)
    }

    /// 管理面分页列表（分页 + 搜索 + 类型筛选），供列表视图增量加载。
    pub(crate) fn list_neurons_page(
        &self,
        page: usize,
        page_size: usize,
        search: Option<&str>,
        kind: NeuronKindFilter,
    ) -> AppResult<NeuronPage> {
        self.store()?
            .list_neurons_page(page, page_size, search, kind)
    }

    /// 管理面设置 / 换绑 / 取消系统类型（system_type 唯一约束由 store 保证）。
    pub(crate) fn set_system_type_for_admin(
        &self,
        id: &str,
        system_type: Option<&str>,
    ) -> AppResult<Neuron> {
        let normalized = system_type.map(str::trim).filter(|s| !s.is_empty());
        // 唯一约束预检查：目标 system_type 已被其他神经元占用时给出友好错误。
        if let Some(target) = normalized {
            if let Some(existing) = self.store()?.get_neuron_by_system_type(target)? {
                if existing.id != id {
                    return Err(AppError::InvalidInput(format!(
                        "system_type {target} is already bound to neuron {}",
                        existing.id
                    )));
                }
            }
        }
        self.store()?.set_system_type(id, normalized)
    }

    /// 记录神经元使用信号（n=1 硬规则短路选中时调用）；忽略失败，不阻塞选择流程。
    pub(crate) fn mark_used_for_assistant(&self, neuron_id: &str) {
        if let Ok(store) = self.store() {
            let _ = store.mark_used(neuron_id);
        }
    }

    /// 活跃数据超容量时，按低价值排序回收最低价值节点（逻辑删除），返回回收数量。
    /// 系统提示词（system_type IS NOT NULL）豁免；幂等，未超容量时返回 0。
    pub(crate) fn recycle_if_over_capacity(&self) -> AppResult<usize> {
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
            phase = PHASE_NEURON_RECYCLE,
            capacity,
            active_before = active,
            victims = victims.len(),
            recycled,
            "recycled low-value neurons over capacity"
        );
        Ok(recycled)
    }
}

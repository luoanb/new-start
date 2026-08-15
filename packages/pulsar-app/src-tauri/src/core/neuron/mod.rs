//! Neuron 领域：知识原子网络 + 选型 + 创建 + 演化 + 系统神经元行为。
//!
//! 目录结构（原 `core/` 扁平文件迁移/拆分而来）：
//! - [`manager`]：`NeuronManager`（Facade），组合 4 个领域服务，公开 API 与旧 `core::neuron_manager` 完全一致。
//! - [`query`]：知识查询与治理（纯 store 读/写）。
//! - [`selection`]：候选池 + 选型（自含生成原语，打破 Creation↔Selection 环）。
//! - [`creation`]：创建/系统神经元/bootstrap 编排。
//! - [`evolution`]：creator 变体状态机。
//! - [`tools`]：AI tool adapters（保留未注册）。
//! - [`store`] / [`model`] / [`config`] / [`spec`]：数据访问层 / 模型调用接口 / 配置读取 / 系统神经元 behavior 管理。

pub mod config;
pub mod creation;
pub mod evolution;
pub mod manager;
pub mod model;
pub mod query;
pub mod selection;
pub mod spec;
pub mod store;
pub mod tools;

/// 统一 Mutex 加锁失败映射（各领域服务复用）。
pub(crate) fn lock_error<T: std::fmt::Display>(error: T) -> crate::core::error::AppError {
    crate::core::error::AppError::StorageError(format!("Failed to lock: {error}"))
}

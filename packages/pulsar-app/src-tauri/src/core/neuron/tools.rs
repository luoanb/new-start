//! AI tool adapters（注册为 System 标签工具，供系统模式会话调用）。
//!
//! 每个 tool 组合持有 `Arc<NeuronManager>`（Facade），通过其公开方法访问多服务。
//! 以前 Agent / Assistant 的 AI 创建流程（insert 驱动：neuron.draft_from_model /
//! neuron.select_one / creator.variant_evolve）不变；本文件是**新增入口**，
//! 与既有流程共用同一底层（create_neuron / select_candidates）。
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::core::{
    error::{AppError, AppResult},
    models::{
        CandidateQuery, CreateNeuronInput, NeuronKindFilter, NeuronUpdate, ToolSource, ToolTag,
    },
    tool_registry::{Tool, ToolRegistry},
};

use super::manager::NeuronManager;

/// 注册神经元管理 System 工具（系统模式会话自动带上；AI 创建流程不变）。
///
/// 全部以 `ToolTag::System + ToolSource::Native` 注册；Native 门禁要求
/// `inserts/<name>.md` 契约手册存在（由 `register_tagged` 内 `InsertCatalog::require` 强制）。
pub fn register_system_tools(registry: &mut ToolRegistry, manager: Arc<NeuronManager>) {
    registry.register_tagged(
        ToolTag::System,
        GetNeuronTool::new(Arc::clone(&manager)),
        ToolSource::Native,
    );
    registry.register_tagged(
        ToolTag::System,
        ListNeuronsTool::new(Arc::clone(&manager)),
        ToolSource::Native,
    );
    registry.register_tagged(
        ToolTag::System,
        UpdateNeuronTool::new(Arc::clone(&manager)),
        ToolSource::Native,
    );
    registry.register_tagged(
        ToolTag::System,
        GetNetworkTool::new(Arc::clone(&manager)),
        ToolSource::Native,
    );
    registry.register_tagged(
        ToolTag::System,
        CreateNeuronTool::new(Arc::clone(&manager)),
        ToolSource::Native,
    );
    registry.register_tagged(
        ToolTag::System,
        SelectNeuronCandidatesTool::new(Arc::clone(&manager)),
        ToolSource::Native,
    );
}

struct GetNeuronTool {
    manager: Arc<NeuronManager>,
}

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

struct ListNeuronsTool {
    manager: Arc<NeuronManager>,
}

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
        "List neurons in pages (page/page_size/search/kind), returns { items, total, has_more }; never returns the full set at once"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "page": {"type": "integer", "minimum": 0, "default": 0, "description": "Page index, 0-based"},
                "page_size": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20, "description": "Items per page (hard cap 100)"},
                "search": {"type": "string", "description": "Optional fuzzy match on desc / id"},
                "kind": {"type": "string", "enum": ["all", "system", "normal"], "default": "all", "description": "Filter by neuron kind"}
            }
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let page = args.get("page").and_then(Value::as_u64).unwrap_or(0) as usize;
        let page_size = args.get("page_size").and_then(Value::as_u64).unwrap_or(20) as usize;
        let search = args.get("search").and_then(Value::as_str).map(str::to_string);
        let kind = args
            .get("kind")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "all".to_string());
        let result = self.manager.list_neurons_page(
            page,
            page_size,
            search.as_deref(),
            NeuronKindFilter::parse(&kind),
        )?;
        serde_json::to_string(&result).map_err(|e| AppError::StorageError(e.to_string()))
    }
}

struct UpdateNeuronTool {
    manager: Arc<NeuronManager>,
}

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
            tool_ids: args.get("tool_ids").and_then(Value::as_array).map(|a| {
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

struct GetNetworkTool {
    manager: Arc<NeuronManager>,
}

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
                "max_depth": {"type": "integer", "minimum": 0, "maximum": 5, "default": 3}
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = required_str(&args, "id")?;
        // clamp：防止过深遍历使结果量级失控。
        let max_depth = args
            .get("max_depth")
            .and_then(Value::as_u64)
            .unwrap_or(3)
            .min(5) as usize;
        serde_json::to_string(&self.manager.get_network(id, max_depth)?)
            .map_err(|e| AppError::StorageError(e.to_string()))
    }
}

struct CreateNeuronTool {
    manager: Arc<NeuronManager>,
}

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

struct SelectNeuronCandidatesTool {
    manager: Arc<NeuronManager>,
}

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
                "n": {"type": "integer", "minimum": 0, "maximum": 50},
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
        // clamp：模型可能传超大 n，结果若全量返回会撑爆上下文。
        let n = n.min(50);
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

fn required_str<'a>(args: &'a Value, key: &str) -> AppResult<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::InvalidInput(format!("Missing: {key}")))
}

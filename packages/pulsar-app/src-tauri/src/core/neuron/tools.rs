//! AI tool adapters（保留未注册，等 insert 引入后再启用）。
//!
//! 每个 tool 组合持有 `Arc<NeuronManager>`（Facade），通过其公开方法访问多服务。
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::core::{
    error::{AppError, AppResult},
    models::{CandidateQuery, CreateNeuronInput, NeuronUpdate},
    tool_registry::Tool,
};

use super::manager::NeuronManager;

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

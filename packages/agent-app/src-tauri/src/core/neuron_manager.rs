use async_trait::async_trait;
use serde_json::Value;
use std::sync::{Arc, Mutex};

use super::{
    error::{AppError, AppResult},
    models::{NeuronUpdate},
    neuron_store::NeuronStore,
    tool_registry::{Tool, ToolRegistry},
};

pub struct NeuronManager {
    store: Arc<Mutex<NeuronStore>>,
}

impl NeuronManager {
    pub fn new(store: Arc<Mutex<NeuronStore>>) -> Self {
        Self { store }
    }

    pub fn register_all(&self, registry: &mut ToolRegistry) {
        registry.register(CreateNeuronTool::new(Arc::clone(&self.store)));
        registry.register(GetNeuronTool::new(Arc::clone(&self.store)));
        registry.register(ListNeuronsTool::new(Arc::clone(&self.store)));
        registry.register(UpdateNeuronTool::new(Arc::clone(&self.store)));
        registry.register(DeleteNeuronTool::new(Arc::clone(&self.store)));
        registry.register(LinkNeuronsTool::new(Arc::clone(&self.store)));
        registry.register(UnlinkNeuronsTool::new(Arc::clone(&self.store)));
        registry.register(GetNetworkTool::new(Arc::clone(&self.store)));
    }
}

// ── CreateNeuronTool ──────────────────────────────────────────────

pub struct CreateNeuronTool {
    store: Arc<Mutex<NeuronStore>>,
}
impl CreateNeuronTool {
    pub fn new(store: Arc<Mutex<NeuronStore>>) -> Self {
        Self { store }
    }
}
#[async_trait]
impl Tool for CreateNeuronTool {
    fn name(&self) -> &str { "create_neuron" }
    fn description(&self) -> &str { "Create a new neuron" }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "desc": {"type": "string", "description": "Short description / label"},
                "content": {"type": "string", "description": "Detailed content"},
                "weight": {"type": "number", "description": "Weight (can be negative). Default: 0"}
            },
            "required": ["desc"]
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let desc = args.get("desc").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing: desc".into()))?;
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let weight = args.get("weight").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let store = self.store.lock().map_err(|e| AppError::StorageError(e.to_string()))?;
        let n = store.create_neuron(desc, content, weight)?;
        Ok(format!("Created neuron '{}' (id: {})", n.desc, n.id))
    }
}

// ── GetNeuronTool ─────────────────────────────────────────────────

pub struct GetNeuronTool {
    store: Arc<Mutex<NeuronStore>>,
}
impl GetNeuronTool {
    pub fn new(store: Arc<Mutex<NeuronStore>>) -> Self { Self { store } }
}
#[async_trait]
impl Tool for GetNeuronTool {
    fn name(&self) -> &str { "get_neuron" }
    fn description(&self) -> &str { "Get details of a neuron by ID, including its connections" }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "Neuron ID"}
            },
            "required": ["id"]
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing: id".into()))?;
        let store = self.store.lock().map_err(|e| AppError::StorageError(e.to_string()))?;
        let n = store.get_neuron(id)?.ok_or_else(|| AppError::ConversationNotFound(format!("Neuron not found: {id}")))?;
        let conns = store.get_connections(id)?;
        let mut lines = vec![
            format!("Neuron: {} (id: {})", n.desc, n.id),
            format!("Content: {}", n.content),
            format!("Weight: {}", n.weight),
        ];
        if !conns.is_empty() {
            lines.push("Connections:".into());
            for c in &conns {
                lines.push(format!("  {} --[{}]--> {}", c.source, c.weight, c.target));
            }
        }
        Ok(lines.join("\n"))
    }
}

// ── ListNeuronsTool ───────────────────────────────────────────────

pub struct ListNeuronsTool {
    store: Arc<Mutex<NeuronStore>>,
}
impl ListNeuronsTool {
    pub fn new(store: Arc<Mutex<NeuronStore>>) -> Self { Self { store } }
}
#[async_trait]
impl Tool for ListNeuronsTool {
    fn name(&self) -> &str { "list_neurons" }
    fn description(&self) -> &str { "List all neurons" }
    fn parameters(&self) -> Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _args: Value) -> AppResult<String> {
        let store = self.store.lock().map_err(|e| AppError::StorageError(e.to_string()))?;
        let neurons = store.list_neurons()?;
        if neurons.is_empty() {
            return Ok("No neurons found.".into());
        }
        let mut lines = vec!["Neurons:".into()];
        for n in &neurons {
            lines.push(format!("  [w:{:+.1}] {} (id: {})", n.weight, n.desc, n.id));
        }
        Ok(lines.join("\n"))
    }
}

// ── UpdateNeuronTool ──────────────────────────────────────────────

pub struct UpdateNeuronTool {
    store: Arc<Mutex<NeuronStore>>,
}
impl UpdateNeuronTool {
    pub fn new(store: Arc<Mutex<NeuronStore>>) -> Self { Self { store } }
}
#[async_trait]
impl Tool for UpdateNeuronTool {
    fn name(&self) -> &str { "update_neuron" }
    fn description(&self) -> &str { "Update a neuron's fields" }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "desc": {"type": "string"},
                "content": {"type": "string"},
                "weight": {"type": "number"}
            },
            "required": ["id"]
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing: id".into()))?;
        let mut update = NeuronUpdate::default();
        if let Some(v) = args.get("desc").and_then(|v| v.as_str()) { update.desc = Some(v.into()); }
        if let Some(v) = args.get("content").and_then(|v| v.as_str()) { update.content = Some(v.into()); }
        if let Some(v) = args.get("weight").and_then(|v| v.as_f64()) { update.weight = Some(v); }
        let store = self.store.lock().map_err(|e| AppError::StorageError(e.to_string()))?;
        let n = store.update_neuron(id, update)?;
        Ok(format!("Updated neuron '{}'", n.desc))
    }
}

// ── DeleteNeuronTool ──────────────────────────────────────────────

pub struct DeleteNeuronTool {
    store: Arc<Mutex<NeuronStore>>,
}
impl DeleteNeuronTool {
    pub fn new(store: Arc<Mutex<NeuronStore>>) -> Self { Self { store } }
}
#[async_trait]
impl Tool for DeleteNeuronTool {
    fn name(&self) -> &str { "delete_neuron" }
    fn description(&self) -> &str { "Delete a neuron and its connections" }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {"id": {"type": "string"}},
            "required": ["id"]
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing: id".into()))?;
        let store = self.store.lock().map_err(|e| AppError::StorageError(e.to_string()))?;
        if store.delete_neuron(id)? {
            Ok(format!("Deleted neuron: {id}"))
        } else {
            Err(AppError::ConversationNotFound(format!("Neuron not found: {id}")))
        }
    }
}

// ── LinkNeuronsTool ───────────────────────────────────────────────

pub struct LinkNeuronsTool {
    store: Arc<Mutex<NeuronStore>>,
}
impl LinkNeuronsTool {
    pub fn new(store: Arc<Mutex<NeuronStore>>) -> Self { Self { store } }
}
#[async_trait]
impl Tool for LinkNeuronsTool {
    fn name(&self) -> &str { "link_neurons" }
    fn description(&self) -> &str { "Create or update a connection between two neurons" }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "source": {"type": "string"},
                "target": {"type": "string"},
                "weight": {"type": "number", "description": "Connection strength. Default: 1.0"}
            },
            "required": ["source", "target"]
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let source = args.get("source").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing: source".into()))?;
        let target = args.get("target").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing: target".into()))?;
        let weight = args.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let store = self.store.lock().map_err(|e| AppError::StorageError(e.to_string()))?;
        store.link(source, target, weight)?;
        Ok(format!("Linked {} --[{}]--> {}", source, weight, target))
    }
}

// ── UnlinkNeuronsTool ─────────────────────────────────────────────

pub struct UnlinkNeuronsTool {
    store: Arc<Mutex<NeuronStore>>,
}
impl UnlinkNeuronsTool {
    pub fn new(store: Arc<Mutex<NeuronStore>>) -> Self { Self { store } }
}
#[async_trait]
impl Tool for UnlinkNeuronsTool {
    fn name(&self) -> &str { "unlink_neurons" }
    fn description(&self) -> &str { "Remove a connection between two neurons" }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "source": {"type": "string"},
                "target": {"type": "string"}
            },
            "required": ["source", "target"]
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let source = args.get("source").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing: source".into()))?;
        let target = args.get("target").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing: target".into()))?;
        let store = self.store.lock().map_err(|e| AppError::StorageError(e.to_string()))?;
        if store.unlink(source, target)? {
            Ok(format!("Removed link {} -> {}", source, target))
        } else {
            Err(AppError::ConversationNotFound(format!("Link not found: {source} -> {target}")))
        }
    }
}

// ── GetNetworkTool ────────────────────────────────────────────────

pub struct GetNetworkTool {
    store: Arc<Mutex<NeuronStore>>,
}
impl GetNetworkTool {
    pub fn new(store: Arc<Mutex<NeuronStore>>) -> Self { Self { store } }
}
#[async_trait]
impl Tool for GetNetworkTool {
    fn name(&self) -> &str { "get_network" }
    fn description(&self) -> &str { "BFS traverse the network from a seed neuron up to max_depth" }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "Seed neuron ID"},
                "max_depth": {"type": "integer", "description": "Max traversal depth. Default: 3"}
            },
            "required": ["id"]
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing: id".into()))?;
        let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
        let store = self.store.lock().map_err(|e| AppError::StorageError(e.to_string()))?;
        let network = store.get_network(id, max_depth)?;
        if network.is_empty() {
            return Ok("No network found.".into());
        }
        let mut lines = vec![format!("Network (depth={}):", max_depth)];
        for n in &network {
            lines.push(format!("  [w:{:+.1}] {} (id: {})", n.weight, n.desc, n.id));
        }
        Ok(lines.join("\n"))
    }
}

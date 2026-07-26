use async_trait::async_trait;
use serde_json::Value;
use std::sync::{Arc, Mutex};

use super::{
    error::{AppError, AppResult},
    models::{ScopeInItem, TopicStatus, TopicUpdate},
    tool_registry::{Tool, ToolRegistry},
    topic_store::TopicStore,
};

/// TopicManager wraps TopicStore and implements Tool for the 5 topic commands.
pub struct TopicManager {
    store: Arc<Mutex<TopicStore>>,
}

impl TopicManager {
    pub fn new(store: Arc<Mutex<TopicStore>>) -> Self {
        Self { store }
    }

    /// Register all topic tools into a ToolRegistry.
    pub fn register_all(&self, registry: &mut ToolRegistry) {
        registry.register(ListTopicsTool::new(Arc::clone(&self.store)));
        registry.register(GetTopicTool::new(Arc::clone(&self.store)));
        registry.register(CreateTopicTool::new(Arc::clone(&self.store)));
        registry.register(UpdateTopicTool::new(Arc::clone(&self.store)));
        registry.register(DeleteTopicTool::new(Arc::clone(&self.store)));
    }
}

// ── Helper to parse TopicStatus from JSON string value ─────────────

fn parse_status(value: &Value) -> AppResult<TopicStatus> {
    let s = value
        .as_str()
        .ok_or_else(|| AppError::InvalidInput("status must be a string".into()))?;
    let json = format!("\"{}\"", s);
    serde_json::from_str(&json).map_err(|_| {
        AppError::InvalidInput(format!(
            "Invalid status: {}. Valid values: todo, in_progress, paused, done, cancelled",
            s
        ))
    })
}

fn status_to_string(status: &TopicStatus) -> String {
    serde_json::to_string(status)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

// ── ListTopicsTool ────────────────────────────────────────────────

pub struct ListTopicsTool {
    store: Arc<Mutex<TopicStore>>,
}

impl ListTopicsTool {
    pub fn new(store: Arc<Mutex<TopicStore>>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for ListTopicsTool {
    fn name(&self) -> &str {
        "list_topics"
    }
    fn description(&self) -> &str {
        "List all topics, optionally filtered by status"
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "description": "Optional status filter: todo, in_progress, paused, done, cancelled",
                    "enum": ["todo", "in_progress", "paused", "done", "cancelled"]
                }
            }
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let status = args
            .get("status")
            .and_then(|v| v.as_str())
            .map(|s| {
                serde_json::from_str::<TopicStatus>(&format!("\"{}\"", s))
                    .map_err(|_| AppError::InvalidInput(format!("Invalid status: {}", s)))
            })
            .transpose()?;

        let store = self
            .store
            .lock()
            .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?;
        let topics = store.list(status)?;

        if topics.is_empty() {
            return Ok("No topics found.".to_string());
        }

        let mut lines = vec!["Topics:".to_string()];
        for t in &topics {
            lines.push(format!(
                "  [{:>3}%] {} - {} (id: {})",
                t.progress,
                t.name,
                status_to_string(&t.status),
                t.id
            ));
        }
        Ok(lines.join("\n"))
    }
}

// ── GetTopicTool ──────────────────────────────────────────────────

pub struct GetTopicTool {
    store: Arc<Mutex<TopicStore>>,
}

impl GetTopicTool {
    pub fn new(store: Arc<Mutex<TopicStore>>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for GetTopicTool {
    fn name(&self) -> &str {
        "get_topic"
    }
    fn description(&self) -> &str {
        "Get details of a single topic by ID"
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Topic ID"
                }
            },
            "required": ["id"]
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing required field: id".into()))?;

        let store = self
            .store
            .lock()
            .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?;
        let topic = store.get(id)?.ok_or_else(|| {
            AppError::ConversationNotFound(format!("Topic not found: {id}"))
        })?;

        let mut lines = vec![
            format!("Topic: {}", topic.name),
            format!("Status: {}", status_to_string(&topic.status)),
            format!("Progress: {}%", topic.progress),
        ];

        if !topic.description.is_empty() {
            lines.push(format!("Description: {}", topic.description));
        }

        if !topic.scope_in.is_empty() {
            lines.push("Scope-in:".to_string());
            for (i, item) in topic.scope_in.iter().enumerate() {
                lines.push(format!("  {}. {}", i + 1, item.goal));
                lines.push(format!("     Done: {}", item.done_contract));
                lines.push(format!("     Status: {}", item.status));
            }
        }

        if let Some(ref extra) = topic.extra {
            lines.push(format!(
                "Extra: {}",
                serde_json::to_string_pretty(extra).unwrap_or_default()
            ));
        }

        Ok(lines.join("\n"))
    }
}

// ── CreateTopicTool ───────────────────────────────────────────────

pub struct CreateTopicTool {
    store: Arc<Mutex<TopicStore>>,
}

impl CreateTopicTool {
    pub fn new(store: Arc<Mutex<TopicStore>>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for CreateTopicTool {
    fn name(&self) -> &str {
        "create_topic"
    }
    fn description(&self) -> &str {
        "Create a new topic"
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Topic name"
                },
                "status": {
                    "type": "string",
                    "description": "Optional initial status. Default: todo",
                    "enum": ["todo", "in_progress", "paused", "done", "cancelled"]
                },
                "description": {
                    "type": "string",
                    "description": "Optional topic description"
                },
                "scope_in": {
                    "type": "array",
                    "description": "Optional list of scope-in items",
                    "items": {
                        "type": "object",
                        "properties": {
                            "goal": {"type": "string"},
                            "done_contract": {"type": "string"},
                            "status": {"type": "string"}
                        }
                    }
                },
                "extra": {
                    "type": "object",
                    "description": "Optional extra data as JSON object"
                }
            },
            "required": ["name"]
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing required field: name".into()))?;

        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let status = match args.get("status") {
            Some(v) => parse_status(v)?,
            None => TopicStatus::Todo,
        };

        let scope_in: Vec<ScopeInItem> = args
            .get("scope_in")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let extra = args.get("extra").cloned();

        let store = self
            .store
            .lock()
            .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?;
        let topic = store.create(name, &description, status, scope_in, extra)?;

        Ok(format!(
            "Created topic '{}' (id: {}) with status {}",
            topic.name,
            topic.id,
            status_to_string(&topic.status)
        ))
    }
}

// ── UpdateTopicTool ───────────────────────────────────────────────

pub struct UpdateTopicTool {
    store: Arc<Mutex<TopicStore>>,
}

impl UpdateTopicTool {
    pub fn new(store: Arc<Mutex<TopicStore>>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for UpdateTopicTool {
    fn name(&self) -> &str {
        "update_topic"
    }
    fn description(&self) -> &str {
        "Update an existing topic by ID. Only provided fields are changed."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Topic ID"
                },
                "name": {
                    "type": "string",
                    "description": "New name"
                },
                "status": {
                    "type": "string",
                    "enum": ["todo", "in_progress", "paused", "done", "cancelled"]
                },
                "description": {
                    "type": "string"
                },
                "scope_in": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "goal": {"type": "string"},
                            "done_contract": {"type": "string"},
                            "status": {"type": "string"}
                        }
                    }
                },
                "progress": {
                    "type": "integer",
                    "description": "Progress 0-100"
                },
                "extra": {
                    "type": "object",
                    "description": "Set extra data. Use extra_clear=true to clear."
                },
                "extra_clear": {
                    "type": "boolean",
                    "description": "Set to true to clear extra field"
                }
            },
            "required": ["id"]
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing required field: id".into()))?;

        let mut update = TopicUpdate::default();

        if let Some(v) = args.get("name").and_then(|v| v.as_str()) {
            update.name = Some(v.to_string());
        }
        if let Some(v) = args.get("status") {
            update.status = Some(parse_status(v)?);
        }
        if let Some(v) = args.get("description").and_then(|v| v.as_str()) {
            update.description = Some(v.to_string());
        }
        if let Some(v) = args.get("scope_in") {
            let items: Vec<ScopeInItem> = serde_json::from_value(v.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid scope_in: {}", e)))?;
            update.scope_in = Some(items);
        }
        if let Some(v) = args.get("progress").and_then(|v| v.as_u64()) {
            update.progress = Some(v.min(100) as u8);
        }
        if args.get("extra_clear").and_then(|v| v.as_bool()).unwrap_or(false) {
            update.extra = Some(None);
        } else if let Some(v) = args.get("extra") {
            update.extra = Some(Some(v.clone()));
        }

        let store = self
            .store
            .lock()
            .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?;
        let topic = store.update(id, update)?;

        Ok(format!(
            "Updated topic '{}' ({}%) - {}",
            topic.name,
            topic.progress,
            status_to_string(&topic.status)
        ))
    }
}

// ── DeleteTopicTool ───────────────────────────────────────────────

pub struct DeleteTopicTool {
    store: Arc<Mutex<TopicStore>>,
}

impl DeleteTopicTool {
    pub fn new(store: Arc<Mutex<TopicStore>>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for DeleteTopicTool {
    fn name(&self) -> &str {
        "delete_topic"
    }
    fn description(&self) -> &str {
        "Delete a topic by ID"
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Topic ID"
                }
            },
            "required": ["id"]
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing required field: id".into()))?;

        let store = self
            .store
            .lock()
            .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?;
        if store.delete(id)? {
            Ok(format!("Deleted topic: {id}"))
        } else {
            Err(AppError::ConversationNotFound(format!(
                "Topic not found: {id}"
            )))
        }
    }
}

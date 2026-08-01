use std::collections::HashMap;

use async_trait::async_trait;
use serde_json;

use super::{
    error::{AppError, AppResult},
    insert_catalog::InsertCatalog,
    models::ToolDefinition,
};
use std::sync::{Arc, Mutex};

/// The Tool trait — implement this to add a new tool a model can call.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value) -> AppResult<String>;
}

/// Registry that holds all available tools.
/// Replaces the old SkillRegistry.
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolBox>,
}

// Workaround: Box<dyn Tool> doesn't implement Clone, so we wrap with an Arc.

struct ToolBox(Arc<dyn Tool>);

impl std::fmt::Debug for ToolBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolBox")
            .field("name", &self.0.name())
            .finish()
    }
}

impl Clone for ToolBox {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Empty registry (external tools are opt-in with inserts).
    pub fn with_defaults() -> Self {
        Self::new()
    }

    /// Empty registry — topic tools are not pre-registered.
    pub fn with_defaults_and_topics(
        _topic_store: Arc<Mutex<super::topic_store::TopicStore>>,
    ) -> Self {
        Self::new()
    }

    /// Empty registry — production assembly does not pre-register tools.
    pub fn with_defaults_and_topics_and_neurons(
        _topic_store: Arc<Mutex<super::topic_store::TopicStore>>,
        _neuron_manager: Arc<super::neuron_manager::NeuronManager>,
        _session_tracker: super::session_tracker::SessionTracker,
    ) -> Self {
        Self::new()
    }

    /// Register a tool. Requires `inserts/<name>.md` (self-describing gate).
    pub fn register(&mut self, tool: impl Tool + 'static) {
        let name = tool.name().to_string();
        let _insert = InsertCatalog::require(&name);
        self.tools.insert(name, ToolBox(Arc::new(tool)));
    }

    pub fn list_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tb| ToolDefinition {
                name: tb.0.name().to_string(),
                description: tb.0.description().to_string(),
                parameters: tb.0.parameters(),
            })
            .collect()
    }

    pub fn definitions_for(&self, tool_ids: &[String]) -> Vec<ToolDefinition> {
        tool_ids
            .iter()
            .filter_map(|id| {
                self.tools.get(id).map(|tb| ToolDefinition {
                    name: tb.0.name().to_string(),
                    description: tb.0.description().to_string(),
                    parameters: tb.0.parameters(),
                })
            })
            .collect()
    }

    pub async fn execute(&self, name: &str, args: serde_json::Value) -> AppResult<String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| AppError::SkillNotFound(name.to_string()))?;
        tool.0.execute(args).await
    }
}

// ─── Built-in tools (unregistered until inserts exist) ─────────────

/// Return the current Unix-millisecond timestamp.
#[allow(dead_code)]
struct GetCurrentTimeTool;

#[async_trait]
impl Tool for GetCurrentTimeTool {
    fn name(&self) -> &str {
        "get_current_time"
    }
    fn description(&self) -> &str {
        "Get the current Unix timestamp in milliseconds"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }
    async fn execute(&self, _args: serde_json::Value) -> AppResult<String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Ok(now.to_string())
    }
}

/// Echo back the input message.
#[allow(dead_code)]
struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "Echo back the input message"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message to echo back"
                }
            },
            "required": ["message"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> AppResult<String> {
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("echo: missing 'message' argument".into()))?;
        Ok(message.to_string())
    }
}

/// Placeholder calculator.
#[allow(dead_code)]
struct CalculateTool;

#[async_trait]
impl Tool for CalculateTool {
    fn name(&self) -> &str {
        "calculate"
    }
    fn description(&self) -> &str {
        "Evaluate a mathematical expression"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "The mathematical expression to evaluate, e.g. '2 + 2'"
                }
            },
            "required": ["expression"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> AppResult<String> {
        let _ = args
            .get("expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::InvalidInput("calculate: missing 'expression' argument".into())
            })?;
        Ok("Calculator not yet implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ProbeTool;

    #[async_trait]
    impl Tool for ProbeTool {
        fn name(&self) -> &str {
            "neuron.select_one"
        }
        fn description(&self) -> &str {
            "probe"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        async fn execute(&self, _args: serde_json::Value) -> AppResult<String> {
            Ok("ok".into())
        }
    }

    #[test]
    fn with_defaults_is_empty() {
        let registry = ToolRegistry::with_defaults();
        assert!(registry.list_definitions().is_empty());
    }

    #[test]
    fn register_requires_insert() {
        let mut registry = ToolRegistry::new();
        registry.register(ProbeTool);
        let defs = registry.list_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "neuron.select_one");
    }

    #[tokio::test]
    async fn execute_unknown_tool_returns_error() {
        let registry = ToolRegistry::new();
        let result = registry.execute("nonexistent", serde_json::json!({})).await;
        assert!(result.is_err());
    }
}

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json;

use super::{
    error::{AppError, AppResult},
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

    /// Create with the three default built-in tools.
    pub fn with_defaults() -> Self {
        let mut reg = Self::new();
        reg.register(GetCurrentTimeTool);
        reg.register(EchoTool);
        reg.register(CalculateTool);
        reg
    }

    /// Create with default tools plus topic management tools.
    pub fn with_defaults_and_topics(topic_store: Arc<Mutex<super::topic_store::TopicStore>>) -> Self {
        let mut reg = Self::with_defaults();
        let manager = super::topic_manager::TopicManager::new(topic_store);
        manager.register_all(&mut reg);
        reg
    }

    /// Create with default tools plus topic, neuron, and runtime management tools.
    pub fn with_defaults_and_topics_and_neurons(
        topic_store: Arc<Mutex<super::topic_store::TopicStore>>,
        neuron_store: Arc<Mutex<super::neuron_store::NeuronStore>>,
        runtime_manager: super::runtime_manager::RuntimeManager,
    ) -> Self {
        let mut reg = Self::with_defaults();
        let topic_manager = super::topic_manager::TopicManager::new(topic_store);
        topic_manager.register_all(&mut reg);
        let neuron_manager = super::neuron_manager::NeuronManager::new(neuron_store);
        neuron_manager.register_all(&mut reg);
        super::runtime_manager::register_runtime_tools(&mut reg, runtime_manager);
        reg
    }

    pub fn register(&mut self, tool: impl Tool + 'static) {
        let name = tool.name().to_string();
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

    pub async fn execute(&self, name: &str, args: serde_json::Value) -> AppResult<String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| AppError::SkillNotFound(name.to_string()))?;
        tool.0.execute(args).await
    }
}

// ─── Built-in tools ───────────────────────────────────────────────

/// Return the current Unix-millisecond timestamp.
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

    #[tokio::test]
    async fn with_defaults_registers_three_tools() {
        let registry = ToolRegistry::with_defaults();
        let defs = registry.list_definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"get_current_time"));
        assert!(names.contains(&"echo"));
        assert!(names.contains(&"calculate"));
    }

    #[tokio::test]
    async fn execute_get_current_time_returns_timestamp() {
        let registry = ToolRegistry::with_defaults();
        let result = registry
            .execute("get_current_time", serde_json::json!({}))
            .await
            .expect("get_current_time should succeed");
        // Should be a numeric string
        assert!(!result.is_empty());
        assert!(result.chars().all(|c| c.is_ascii_digit()));
    }

    #[tokio::test]
    async fn execute_echo_returns_input() {
        let registry = ToolRegistry::with_defaults();
        let result = registry
            .execute("echo", serde_json::json!({"message": "hello world"}))
            .await
            .expect("echo should succeed");
        assert_eq!(result, "hello world");
    }

    #[tokio::test]
    async fn execute_unknown_tool_returns_error() {
        let registry = ToolRegistry::with_defaults();
        let result = registry
            .execute("nonexistent", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }
}

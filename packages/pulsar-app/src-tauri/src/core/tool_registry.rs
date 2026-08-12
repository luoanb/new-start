use std::collections::HashMap;

use async_trait::async_trait;
use serde_json;

use super::{
    error::{AppError, AppResult},
    insert_catalog::InsertCatalog,
    models::{ToolDefinition, ToolSource},
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

struct ToolBox {
    tool: Arc<dyn Tool>,
    source: ToolSource,
}

impl ToolBox {
    fn new(tool: Arc<dyn Tool>, source: ToolSource) -> Self {
        Self { tool, source }
    }
}

impl std::fmt::Debug for ToolBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolBox")
            .field("name", &self.tool.name())
            .field("source", &self.source)
            .finish()
    }
}

impl Clone for ToolBox {
    fn clone(&self) -> Self {
        Self {
            tool: self.tool.clone(),
            source: self.source,
        }
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

    /// Register a project-owned native tool. Requires `inserts/<name>.md`
    /// (self-describing gate).
    pub fn register(&mut self, tool: impl Tool + 'static) {
        self.register_source(tool, ToolSource::Native);
    }

    /// Register a tool with an explicit source.
    /// - `Native`: keeps the `inserts/<name>.md` gate.
    /// - `Config` / `Mcp`: dynamic channels are self-describing (schema is the
    ///   contract) and are exempt from the insert gate.
    pub fn register_source(&mut self, tool: impl Tool + 'static, source: ToolSource) {
        let name = tool.name().to_string();
        if source == ToolSource::Native {
            let _insert = InsertCatalog::require(&name);
        }
        self.tools
            .insert(name, ToolBox::new(Arc::new(tool), source));
    }

    /// 按名取出工具引用（不持锁语义：调用方在读锁守卫内 clone 后即可释放锁，
    /// 再 await `execute`，避免读锁跨 await）。
    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).map(|tb| Arc::clone(&tb.tool))
    }

    pub fn list_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tb| ToolDefinition {
                name: tb.tool.name().to_string(),
                description: tb.tool.description().to_string(),
                parameters: tb.tool.parameters(),
                source: tb.source,
            })
            .collect()
    }

    pub fn definitions_for(&self, tool_ids: &[String]) -> Vec<ToolDefinition> {
        tool_ids
            .iter()
            .filter_map(|id| {
                self.tools.get(id).map(|tb| ToolDefinition {
                    name: tb.tool.name().to_string(),
                    description: tb.tool.description().to_string(),
                    parameters: tb.tool.parameters(),
                    source: tb.source,
                })
            })
            .collect()
    }

    pub async fn execute(&self, name: &str, args: serde_json::Value) -> AppResult<String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| AppError::SkillNotFound(name.to_string()))?;
        tool.tool.execute(args).await
    }
}

// ─── Built-in tools (unregistered until inserts exist) ─────────────

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

    struct NoInsertTool;

    #[async_trait]
    impl Tool for NoInsertTool {
        fn name(&self) -> &str {
            "dynamic.no_insert"
        }
        fn description(&self) -> &str {
            "no insert tool"
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
        assert_eq!(defs[0].source, ToolSource::Native);
    }

    #[test]
    fn register_source_exempts_dynamic_sources_from_insert() {
        assert!(!InsertCatalog::exists("dynamic.no_insert"));
        let mut registry = ToolRegistry::new();
        // 无 insert 的工具经 Config/Mcp 来源注册应成功（豁免门禁）。
        registry.register_source(NoInsertTool, ToolSource::Config);
        registry.register_source(NoInsertTool, ToolSource::Mcp);
        let defs = registry.list_definitions();
        assert_eq!(defs.len(), 1, "同名字工具应覆盖注册");
        assert_eq!(defs[0].source, ToolSource::Mcp);
    }

    #[tokio::test]
    async fn execute_unknown_tool_returns_error() {
        let registry = ToolRegistry::new();
        let result = registry.execute("nonexistent", serde_json::json!({})).await;
        assert!(result.is_err());
    }
}

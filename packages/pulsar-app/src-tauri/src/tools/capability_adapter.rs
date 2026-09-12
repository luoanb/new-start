//! 能力适配器：注册表驱动的 [`CapabilityExecutor`] 真实实现。
//!
//! 单调用执行语义（授权 / 失败转结果 / 截断）的唯一归属，执行面与端口共用同一实现。

use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use crate::core::context_safety::cap_tool_result;
use crate::core::error::{AppError, AppResult};
use crate::core::log_phase::PHASE_ROUND_EXECUTE;
use crate::core::models::AuthorizedToolCall;
use crate::core::round_contract::CapabilityExecutor;
use crate::core::round_types::ToolResult;
use crate::tools::tool_registry::ToolRegistry;

/// 能力执行器（真实实现，架构设计 §3.4）：注册表驱动的单调用执行。
///
/// 工具失败不冒泡——失败信息作为结果内容回传模型（声明与结果成对 / 独立落库）；
/// 未知工具返回 `Err`（授权校验先行，此处为纵深防御）；结果统一经上下文安全截断。
pub struct CapabilityAdapter {
    tool_registry: Arc<RwLock<ToolRegistry>>,
    tool_result_max_chars: usize,
}

impl CapabilityAdapter {
    pub fn new(tool_registry: Arc<RwLock<ToolRegistry>>, tool_result_max_chars: usize) -> Self {
        Self {
            tool_registry,
            tool_result_max_chars,
        }
    }
}

#[async_trait]
impl CapabilityExecutor for CapabilityAdapter {
    async fn execute(&self, call: AuthorizedToolCall) -> AppResult<ToolResult> {
        let tool = self
            .tool_registry
            .read()
            .expect("tool registry lock should not be poisoned")
            .get_tool(&call.name)
            .ok_or_else(|| AppError::SkillNotFound(call.name.clone()))?;
        tracing::info!(
            phase = PHASE_ROUND_EXECUTE,
            tool = %call.name,
            args_len = call.arguments.to_string().len(),
            "executing tool"
        );
        let result = match tool.execute(call.arguments.clone()).await {
            Ok(result) => result,
            Err(error) => {
                // 工具失败不阻塞整轮：把失败信息作为工具结果回传给模型，
                // 由模型决定重试、换工具或直接基于失败继续作答。
                let message = format!("[tool:{}] 工具调用失败：{error}", call.name);
                tracing::warn!(
                    phase = PHASE_ROUND_EXECUTE,
                    tool = %call.name,
                    error = %error,
                    "tool failed; error passed back to model"
                );
                message
            }
        };
        tracing::info!(
            phase = PHASE_ROUND_EXECUTE,
            tool = %call.name,
            result_len = result.len(),
            "tool executed"
        );
        // 统一上下文安全兜底：任何工具结果超上限 → head/tail 截断 + 提示。
        // 落库点执行（结果随后既落库又拼进本轮输出，一处截断两头受益）。
        let result = cap_tool_result(&call.name, result, self.tool_result_max_chars);
        Ok(ToolResult {
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            content: result,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::core::context_safety::DEFAULT_TOOL_RESULT_MAX_CHARS;
    use crate::core::error::AppError;
    use crate::core::models::{AuthorizedToolCall, ToolSource};
    use crate::tools::tool_registry::{Tool, ToolRegistry};

    struct ContractEchoTool;

    #[async_trait]
    impl Tool for ContractEchoTool {
        fn name(&self) -> &str {
            "contract-echo"
        }
        fn description(&self) -> &str {
            "echo back text"
        }
        fn parameters(&self) -> serde_json::Value {
            json!({"type":"object","properties":{"text":{"type":"string"}}})
        }
        async fn execute(&self, args: serde_json::Value) -> AppResult<String> {
            match args.get("text").and_then(|v| v.as_str()) {
                Some("boom") => Err(AppError::RuntimeError("boom".into())),
                Some(text) => Ok(format!("echo:{text}")),
                None => Ok("echo:".into()),
            }
        }
    }

    fn tool_executor() -> CapabilityAdapter {
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        registry
            .write()
            .unwrap()
            .register_source(ContractEchoTool, ToolSource::Config);
        CapabilityAdapter::new(registry, DEFAULT_TOOL_RESULT_MAX_CHARS)
    }

    #[tokio::test]
    async fn capability_executor_pairs_success_and_failure_results() {
        let executor = tool_executor();
        // 成功：结果与调用 id 配对。
        let ok = executor
            .execute(AuthorizedToolCall {
                id: "c1".into(),
                name: "contract-echo".into(),
                arguments: json!({"text": "hi"}),
            })
            .await
            .unwrap();
        assert_eq!(ok.tool_call_id, "c1");
        assert_eq!(ok.tool_name, "contract-echo");
        assert_eq!(ok.content, "echo:hi");
        // 失败不冒泡：失败信息作为结果内容，配对保留（§3.4 不能静默丢弃失败）。
        let failed = executor
            .execute(AuthorizedToolCall {
                id: "c2".into(),
                name: "contract-echo".into(),
                arguments: json!({"text": "boom"}),
            })
            .await
            .unwrap();
        assert_eq!(failed.tool_call_id, "c2");
        assert!(
            failed.content.contains("工具调用失败"),
            "content: {}",
            failed.content
        );
        // 未知工具：Err（授权校验先行，此处为纵深防御）。
        assert!(
            executor
                .execute(AuthorizedToolCall {
                    id: "c3".into(),
                    name: "nope".into(),
                    arguments: json!({}),
                })
                .await
                .is_err()
        );
    }
}

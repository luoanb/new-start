//! 注入点契约：`InjectPointId` 规格卡 + `HookDef`（注册单元）+ `HookRegistry`（注册与执行）。
//!
//! 设计原则（多轮讨论收敛，用户拍板）：
//! - **注入点即类型**：hook 的能力边界由注入点（挂载位置）规格卡写死，无独立 kind 分类；
//!   挂在哪决定了它能消费什么上下文、能做什么操作。
//! - **放权**：上下文尽量给（每个注入点丢当前轮完整 `RoundContext`）、操作权限尽量给
//!   （能 `&mut` 就 `&mut`，不设字段级权限；`ModelCallResponse` / `Vec<ToolResultItem>`
//!   两个局部产物就近作第二 `&mut` 参数）、边界只画在当前轮（不跨会话 / 不跨轮 / 不给全局）。
//! - **失败策略梯度**：越靠前越硬（IP-1=fail）、越靠后越软（IP-2~IP-5=ignore）——
//!   数据一旦入库（persist_input 后），中止会丢轮次产物。
//! - 契约从核心流程 5 步的上下文推导，不参考既有 hook 实现；业务层语义由上层装配期注册。

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::core::{
    conversation_runner::RoundContext,
    error::AppResult,
    models::ModelCallResponse,
    round_types::ToolResultItem,
};

/// handler 返回的 async future：`run_round` 本身是 async，注入点分发天然在 async 上下文。
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 注入点：名字 = 「核心流程第几步之后」，读者一眼看懂挂在哪。
/// 文档讨论时可用简称 IP-1~IP-5（对应顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InjectPointId {
    /// 核心步① load_context 之后（IP-1）
    AfterLoadContext,
    /// 核心步② assemble + persist_input 之后（IP-2）
    AfterPersistInput,
    /// 核心步③ call_model 之后（IP-3）
    AfterCallModel,
    /// 核心步④ execute_tools 之后（IP-4）
    AfterExecuteTools,
    /// 核心步⑤ persist_outcome 之后（IP-5）
    AfterPersistOutcome,
}

impl InjectPointId {
    pub const fn as_str(&self) -> &'static str {
        match self {
            InjectPointId::AfterLoadContext => "after_load_context",
            InjectPointId::AfterPersistInput => "after_persist_input",
            InjectPointId::AfterCallModel => "after_call_model",
            InjectPointId::AfterExecuteTools => "after_execute_tools",
            InjectPointId::AfterPersistOutcome => "after_persist_outcome",
        }
    }
}

/// 每个变体对应一个注入点；第一参 = 当前轮完整上下文（`&mut` 可改 / `&` 只读），
/// 后续参 = 就近局部产物（不在 `RoundContext` 里的 call_model / execute_tools 输出）。
/// 是否执行由 handler 内部自行判断（无独立 guard 机制）。
pub enum HookHandler {
    /// AfterLoadContext：整轮上下文全量可改（选型在此改 messages / state）。
    AfterLoadContext(Box<dyn Fn(&mut RoundContext) -> BoxFuture<'_, AppResult<()>> + Send + Sync>),
    /// AfterPersistInput：wire 已落库，改 ctx.messages 只影响本次发送、不动真相源。
    AfterPersistInput(Box<dyn Fn(&mut RoundContext) -> BoxFuture<'_, AppResult<()>> + Send + Sync>),
    /// AfterCallModel：追加 call_model 返回值，可改写响应 / 拦截工具调用。
    AfterCallModel(
        Box<
            dyn for<'a> Fn(&'a mut RoundContext, &'a mut ModelCallResponse) -> BoxFuture<'a, AppResult<()>>
                + Send
                + Sync,
        >,
    ),
    /// AfterExecuteTools：追加 execute_tools 产出的工具结果，可改写 / 丢弃。
    AfterExecuteTools(
        Box<
            dyn for<'a> Fn(&'a mut RoundContext, &'a mut Vec<ToolResultItem>) -> BoxFuture<'a, AppResult<()>>
                + Send
                + Sync,
        >,
    ),
    /// AfterPersistOutcome：产物已落库，只读整轮上下文；落账本等副作用由 hook 自办。
    AfterPersistOutcome(Box<dyn Fn(&RoundContext) -> BoxFuture<'_, AppResult<()>> + Send + Sync>),
}

/// 注册单元：`id`（唯一标识，重复注册拒绝）+ `label`（可观测 label，账本 / 日志）+ 挂载点
/// （决定 handler 变体 + 失败策略）。mandatory / guard 机制已移除（无实例，需要时再引入）。
pub struct HookDef {
    pub id: &'static str,
    pub label: &'static str,
    pub inject_point: InjectPointId,
    pub handler: HookHandler,
}

/// 注册失败：同 id 重复注册。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterError {
    DuplicateId(String),
}

impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegisterError::DuplicateId(id) => write!(f, "hook id already registered: {id}"),
        }
    }
}

impl std::error::Error for RegisterError {}

/// 卸载失败：id 不存在。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnregisterError {
    NotFound(String),
}

impl fmt::Display for UnregisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnregisterError::NotFound(id) => write!(f, "hook id not registered: {id}"),
        }
    }
}

impl std::error::Error for UnregisterError {}

struct RegisteredHook {
    def: Arc<HookDef>,
}

/// 注入点注册表：按注入点分组、组内按注册顺序执行；`&mut` 直接链式传值——
/// runner 按注册顺序调用，后注册 hook 自然看到前注册 hook 的修改，可继续改写。
///
/// 内部 `Mutex`：注册 / 卸载 / 执行均为 `&self`（runner 以 `Arc<HookRegistry>` 共享，
/// 装配期与执行期并发访问；run_* 内部快照 hook 列表后锁外 await，不跨 await 持锁）。
#[derive(Default)]
pub struct HookRegistry {
    inner: Mutex<HashMap<InjectPointId, Vec<RegisteredHook>>>,
}

impl fmt::Debug for HookRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ids: Vec<_> = self
            .inner
            .lock()
            .expect("hook registry lock")
            .values()
            .flatten()
            .map(|h| h.def.id)
            .collect();
        f.debug_struct("HookRegistry").field("ids", &ids).finish()
    }
}

impl HookRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册：同 id 重复 → `Err(DuplicateId)`；成功挂到 `def.inject_point` 组内末尾（执行顺序）。
    pub fn register(&self, def: HookDef) -> Result<(), RegisterError> {
        let mut hooks = self.inner.lock().expect("hook registry lock");
        if hooks.values().flatten().any(|h| h.def.id == def.id) {
            return Err(RegisterError::DuplicateId(def.id.to_string()));
        }
        hooks
            .entry(def.inject_point)
            .or_default()
            .push(RegisteredHook { def: Arc::new(def) });
        Ok(())
    }

    /// 卸载：按 id 移除（不区分注入点）。
    pub fn unregister(&self, id: &str) -> Result<(), UnregisterError> {
        let mut hooks = self.inner.lock().expect("hook registry lock");
        for group in hooks.values_mut() {
            if let Some(pos) = group.iter().position(|h| h.def.id == id) {
                group.remove(pos);
                return Ok(());
            }
        }
        Err(UnregisterError::NotFound(id.to_string()))
    }

    /// 查询：id 是否已注册（装配幂等 / 单测断言用）。
    pub fn is_registered(&self, id: &str) -> bool {
        let hooks = self.inner.lock().expect("hook registry lock");
        hooks.values().flatten().any(|h| h.def.id == id)
    }

    pub fn len(&self) -> usize {
        let hooks = self.inner.lock().expect("hook registry lock");
        hooks.values().map(|v| v.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 组快照：锁内取 Arc 引用列表，锁外 await（std MutexGuard 不可跨 await 持有）。
    fn snapshot(&self, point: InjectPointId) -> Vec<Arc<HookDef>> {
        let hooks = self.inner.lock().expect("hook registry lock");
        hooks
            .get(&point)
            .map(|group| group.iter().map(|h| Arc::clone(&h.def)).collect())
            .unwrap_or_default()
    }

    /// IP-1：load_context 后。**fail 策略**——最早点未落库，Err 上抛中止本轮。
    ///
    /// 会话切换响应：hook 可改写 `ctx.session_id`（路由决策，如 assistant 课题切换会话）；
    /// 每次 hook 执行后若 session 变化，调用 `on_session_switch`（runner 传入 reload）重载
    /// 新会话上下文，后续 hooks 基于最终会话数据执行（选型等依赖新会话 seed/state/messages）。
    pub async fn run_after_load_context(
        &self,
        ctx: &mut RoundContext,
        on_session_switch: impl Fn(&mut RoundContext) -> AppResult<()>,
    ) -> AppResult<()> {
        let hooks = self.snapshot(InjectPointId::AfterLoadContext);
        let mut last_session = ctx.session_id.clone();
        for def in hooks {
            if let HookHandler::AfterLoadContext(f) = &def.handler {
                f(ctx).await?;
                if ctx.session_id != last_session {
                    on_session_switch(ctx)?;
                    last_session = ctx.session_id.clone();
                }
            }
        }
        Ok(())
    }

    /// IP-2：persist_input 后、call_model 前。**ignore 策略**——Err 按原 wire 发送。
    pub async fn run_after_persist_input(&self, ctx: &mut RoundContext) {
        for def in self.snapshot(InjectPointId::AfterPersistInput) {
            if let HookHandler::AfterPersistInput(f) = &def.handler {
                if let Err(e) = f(ctx).await {
                    tracing::warn!(
                        hook_id = def.id,
                        error = %e,
                        "after_persist_input hook failed; sending original wire"
                    );
                }
            }
        }
    }

    /// IP-3：call_model 后、execute_tools 前。**ignore 策略**——Err 用原响应继续。
    pub async fn run_after_call_model(&self, ctx: &mut RoundContext, response: &mut ModelCallResponse) {
        for def in self.snapshot(InjectPointId::AfterCallModel) {
            if let HookHandler::AfterCallModel(f) = &def.handler {
                if let Err(e) = f(ctx, response).await {
                    tracing::warn!(
                        hook_id = def.id,
                        error = %e,
                        "after_call_model hook failed; using original response"
                    );
                }
            }
        }
    }

    /// IP-4：execute_tools 后、persist_outcome 前。**ignore 策略**——Err 用原工具结果。
    pub async fn run_after_execute_tools(
        &self,
        ctx: &mut RoundContext,
        results: &mut Vec<ToolResultItem>,
    ) {
        for def in self.snapshot(InjectPointId::AfterExecuteTools) {
            if let HookHandler::AfterExecuteTools(f) = &def.handler {
                if let Err(e) = f(ctx, results).await {
                    tracing::warn!(
                        hook_id = def.id,
                        error = %e,
                        "after_execute_tools hook failed; using original tool results"
                    );
                }
            }
        }
    }

    /// IP-5：persist_outcome 后。**ignore 策略**——产物已入库，Err 不影响本轮。
    pub async fn run_after_persist_outcome(&self, ctx: &RoundContext) {
        for def in self.snapshot(InjectPointId::AfterPersistOutcome) {
            if let HookHandler::AfterPersistOutcome(f) = &def.handler {
                if let Err(e) = f(ctx).await {
                    tracing::warn!(
                        hook_id = def.id,
                        error = %e,
                        "after_persist_outcome hook failed"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok() -> BoxFuture<'static, AppResult<()>> {
        Box::pin(async { Ok(()) })
    }

    fn boom() -> BoxFuture<'static, AppResult<()>> {
        Box::pin(async { Err(crate::core::error::AppError::RuntimeError("boom".into())) })
    }

    fn ctx(seed: Option<&str>) -> RoundContext {
        RoundContext {
            session_id: "s-1".into(),
            mode: crate::core::models::ConversationMode::Chat,
            seed: seed.map(|s| crate::core::round_types::SessionSeed::Neuron(s.into())),
            state: crate::core::round_types::SessionState::default(),
            messages: Vec::new(),
            model_input: String::new(),
            model: crate::core::models::ChatModelSelection::new("test-provider", "test-model"),
            tool_override: None,
            trigger: crate::core::conversation_runner::RoundTriggerKind::User,
            topic_id: None,
            reselect: true,
            nudge_persist: false,
            selected_neuron: None,
            outcome: None,
        }
    }

    fn sample_response() -> ModelCallResponse {
        ModelCallResponse {
            provider_id: "p".into(),
            model_id: "m".into(),
            output: "hello".into(),
            tool_calls: None,
            finish_reason: "stop".into(),
            reasoning: None,
        }
    }

    #[tokio::test]
    async fn register_runs_in_order_and_chains() {
        let registry = HookRegistry::new();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        for (id, tag) in [("h1", "first"), ("h2", "second")] {
            let order = std::sync::Arc::clone(&order);
            let calls = std::sync::Arc::clone(&calls);
            registry
                .register(HookDef {
                    id,
                    label: tag,
                    inject_point: InjectPointId::AfterPersistInput,
                    handler: HookHandler::AfterPersistInput(Box::new(move |_ctx| {
                        let calls = std::sync::Arc::clone(&calls);
                        let order = std::sync::Arc::clone(&order);
                        Box::pin(async move {
                            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            order.lock().unwrap().push(tag.to_string());
                            Ok(())
                        })
                    })),
                })
                .unwrap();
        }
        let mut c = ctx(None);
        registry.run_after_persist_input(&mut c).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(*order.lock().unwrap(), ["first", "second"]);
    }

    #[test]
    fn duplicate_id_rejected() {
        let registry = HookRegistry::new();
        let make = || HookDef {
            id: "dup",
            label: "dup",
            inject_point: InjectPointId::AfterLoadContext,
            handler: HookHandler::AfterLoadContext(Box::new(|_| ok())),
        };
        assert!(registry.register(make()).is_ok());
        assert_eq!(
            registry.register(make()),
            Err(RegisterError::DuplicateId("dup".into()))
        );
    }

    #[test]
    fn unregister_removes_by_id() {
        let registry = HookRegistry::new();
        registry
            .register(HookDef {
                id: "x",
                label: "x",
                inject_point: InjectPointId::AfterCallModel,
                handler: HookHandler::AfterCallModel(Box::new(|_, _| ok())),
            })
            .unwrap();
        assert!(registry.unregister("x").is_ok());
        assert_eq!(registry.unregister("x"), Err(UnregisterError::NotFound("x".into())));
    }

    #[tokio::test]
    async fn load_context_failure_propagates() {
        let registry = HookRegistry::new();
        registry
            .register(HookDef {
                id: "fail",
                label: "fail",
                inject_point: InjectPointId::AfterLoadContext,
                handler: HookHandler::AfterLoadContext(Box::new(|_| boom())),
            })
            .unwrap();
        let mut c = ctx(None);
        assert!(registry
            .run_after_load_context(&mut c, |_| Ok(()))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn ignore_points_swallow_errors() {
        let registry = HookRegistry::new();
        registry
            .register(HookDef {
                id: "p",
                label: "p",
                inject_point: InjectPointId::AfterPersistInput,
                handler: HookHandler::AfterPersistInput(Box::new(|_| boom())),
            })
            .unwrap();
        let mut c = ctx(None);
        registry.run_after_persist_input(&mut c).await; // 不 panic、不返回 Err
        assert_eq!(c.session_id, "s-1");
    }

    #[tokio::test]
    async fn call_model_hook_can_rewrite_response() {
        let registry = HookRegistry::new();
        registry
            .register(HookDef {
                id: "rewrite",
                label: "rewrite",
                inject_point: InjectPointId::AfterCallModel,
                handler: HookHandler::AfterCallModel(Box::new(|_ctx, resp| {
                    Box::pin(async move {
                        resp.output = "rewritten".into();
                        Ok(())
                    })
                })),
            })
            .unwrap();
        let mut c = ctx(None);
        let mut resp = sample_response();
        registry.run_after_call_model(&mut c, &mut resp).await;
        assert_eq!(resp.output, "rewritten");
    }

    #[tokio::test]
    async fn execute_tools_hook_can_drop_results() {
        let registry = HookRegistry::new();
        registry
            .register(HookDef {
                id: "drop",
                label: "drop",
                inject_point: InjectPointId::AfterExecuteTools,
                handler: HookHandler::AfterExecuteTools(Box::new(|_ctx, results| {
                    Box::pin(async move {
                        results.clear();
                        Ok(())
                    })
                })),
            })
            .unwrap();
        let mut c = ctx(None);
        let mut results = vec![ToolResultItem {
            tool_call_id: "t1".into(),
            tool_name: "tool".into(),
            content: "ok".into(),
        }];
        registry.run_after_execute_tools(&mut c, &mut results).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn persist_outcome_hook_sees_readonly_ctx() {
        let registry = HookRegistry::new();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let seen2 = std::sync::Arc::clone(&seen);
        registry
            .register(HookDef {
                id: "audit",
                label: "audit",
                inject_point: InjectPointId::AfterPersistOutcome,
                handler: HookHandler::AfterPersistOutcome(Box::new(move |ctx| {
                    let seen2 = std::sync::Arc::clone(&seen2);
                    Box::pin(async move {
                        *seen2.lock().unwrap() = Some(ctx.session_id.clone());
                        Ok(())
                    })
                })),
            })
            .unwrap();
        let c = ctx(Some("n-1"));
        registry.run_after_persist_outcome(&c).await;
        assert_eq!(seen.lock().unwrap().as_deref(), Some("s-1"));
    }

    #[tokio::test]
    async fn empty_registry_is_noop() {
        let registry = HookRegistry::new();
        assert!(registry.is_empty());
        let mut c = ctx(None);
        registry
            .run_after_load_context(&mut c, |_| Ok(()))
            .await
            .unwrap();
        registry.run_after_persist_input(&mut c).await;
        let mut resp = sample_response();
        registry.run_after_call_model(&mut c, &mut resp).await;
        let mut results = Vec::new();
        registry.run_after_execute_tools(&mut c, &mut results).await;
        registry.run_after_persist_outcome(&c).await;
    }

    #[tokio::test]
    async fn session_switch_triggers_reload_then_runs_remaining_hooks() {
        let registry = HookRegistry::new();
        // hook1：路由切换会话（模拟 assistant match_topic switch）。
        registry
            .register(HookDef {
                id: "route",
                label: "route",
                inject_point: InjectPointId::AfterLoadContext,
                handler: HookHandler::AfterLoadContext(Box::new(|ctx| {
                    let target = ctx.session_id.clone() + "-b";
                    Box::pin(async move {
                        ctx.session_id = target;
                        Ok(())
                    })
                })),
            })
            .unwrap();
        // hook2：基于 reload 后的最终会话数据执行（模拟选型 hook）。
        let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let seen2 = std::sync::Arc::clone(&seen);
        registry
            .register(HookDef {
                id: "selection",
                label: "selection",
                inject_point: InjectPointId::AfterLoadContext,
                handler: HookHandler::AfterLoadContext(Box::new(move |ctx| {
                    let seen2 = std::sync::Arc::clone(&seen2);
                    Box::pin(async move {
                        *seen2.lock().unwrap() = ctx.session_id.clone();
                        Ok(())
                    })
                })),
            })
            .unwrap();
        let mut c = ctx(None);
        let reload_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let reload_calls2 = std::sync::Arc::clone(&reload_calls);
        registry
            .run_after_load_context(&mut c, |ctx| {
                reload_calls2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                // runner 的 reload 语义：重载新会话数据（此处覆盖 seed 以验证后续 hook 可见）。
                ctx.seed = Some(crate::core::round_types::SessionSeed::Neuron("reloaded".into()));
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(c.session_id, "s-1-b", "hook1 切换会话生效");
        assert_eq!(
            reload_calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "会话切换恰好触发一次 reload"
        );
        assert_eq!(
            seen.lock().unwrap().as_str(),
            "s-1-b",
            "hook2 应看到 reload 后的最终会话"
        );
        assert!(matches!(
            c.seed,
            Some(crate::core::round_types::SessionSeed::Neuron(ref n)) if n == "reloaded"
        ));
    }
}

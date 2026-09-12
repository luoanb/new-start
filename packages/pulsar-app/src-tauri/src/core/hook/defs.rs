//! 注入点契约：`InjectPointId` 规格卡 + `HookDef`（**定义**）+ `HookRegistry`（**注册 + 开启 + 周期判定 + 执行**）。
//!
//! 三阶段分离：定义（构造 `HookDef`）/ 注册（`register`，默认关闭）/ 开启（`set_enabled`）。
//!
//! 周期（调度）语义见 [`super::cycle`]：
//! - 周期 = **调度**（何时调用动作）；判定由注册表在调用 handler **前**统一求值
//!   （[`super::cycle::gate_check`]），未命中记 skip 日志并跳过；
//! - 判定素材只来自 [`super::cycle::CycleFacts::derive`]（由 `RoundContext` 派生），
//!   业务状态不进周期条件；
//! - 可调项声明（[`CycleParamSpec`]）由动作提供方给出，`usage = Internal` 的项由 handler
//!   通过 [`HookRegistry::param_of`] 读取；
//! - **无硬保护**：任何动作都可 `set_enabled(false)`；`disable_hint` 只是面板提示信息。
//!
//! 设计原则（多轮讨论收敛，用户拍板）：
//! - **注入点即类型**：hook 的能力边界由注入点（挂载位置）规格卡写死，无独立 kind 分类；
//! - **放权**：上下文尽量给、操作权限尽量给、边界只画在当前轮；
//! - **失败策略梯度**：越靠前越硬（IP-1=fail）、越靠后越软（IP-2~IP-5=ignore）。

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::cycle::{
    gate_check, validate_value, CycleFacts, CycleParamKind, CycleParamSpec, CycleParamUsage,
    CycleValue,
};
use crate::core::log_phase::PHASE_HOOK_CYCLE_GATE;
use crate::core::{
    error::AppResult,
    models::ModelResponse,
    round_service::RoundContext,
    round_types::ToolResult,
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
/// 是否执行由注册表按 `enabled` + 周期判定决定（handler 内不再有周期判定）。
pub enum HookHandler {
    /// AfterLoadContext：整轮上下文全量可改（选型在此改 messages / state）。
    AfterLoadContext(Box<dyn Fn(&mut RoundContext) -> BoxFuture<'_, AppResult<()>> + Send + Sync>),
    /// AfterPersistInput：wire 已落库，改 ctx.messages 只影响本次发送、不动真相源。
    AfterPersistInput(Box<dyn Fn(&mut RoundContext) -> BoxFuture<'_, AppResult<()>> + Send + Sync>),
    /// AfterCallModel：追加 call_model 返回值，可改写响应 / 拦截工具调用。
    AfterCallModel(
        Box<
            dyn for<'a> Fn(&'a mut RoundContext, &'a mut ModelResponse) -> BoxFuture<'a, AppResult<()>>
                + Send
                + Sync,
        >,
    ),
    /// AfterExecuteTools：追加 execute_tools 产出的工具结果，可改写 / 丢弃。
    AfterExecuteTools(
        Box<
            dyn for<'a> Fn(&'a mut RoundContext, &'a mut Vec<ToolResult>) -> BoxFuture<'a, AppResult<()>>
                + Send
                + Sync,
        >,
    ),
    /// AfterPersistOutcome：产物已落库，只读整轮上下文；落账本等副作用由 hook 自办。
    AfterPersistOutcome(Box<dyn Fn(&RoundContext) -> BoxFuture<'_, AppResult<()>> + Send + Sync>),
}

/// 注册单元：`spec`（身份 / 分组 / 关停提示 / 周期可调项）+ `handler`（实现）。
pub struct HookDef {
    pub id: &'static str,
    pub label: &'static str,
    pub inject_point: InjectPointId,
    pub handler: HookHandler,
    /// 面板分组（**开放 i18n key**，核心不设枚举）。
    pub group: &'static str,
    /// 关停风险提示（i18n key）；`None` = 无提示。
    /// **不硬保护**：不参与分发、不参与命令校验，仅由面板展示。
    pub disable_hint: Option<&'static str>,
    /// 周期可调项声明（默认值 = 现状行为值）。
    pub cycle_params: &'static [CycleParamSpec],
}

/// 注册 / 开关 / 取值失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterError {
    /// 同 id 重复注册。
    DuplicateId(String),
    /// 目标 id 未注册。
    UnknownId(String),
    /// 目标 param key 未在该动作的声明中（`"<id>.<key>"`）。
    UnknownParam(String),
    /// 取值不符合声明（形态 / 候选值 / 范围）。
    InvalidParam(String),
}

impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegisterError::DuplicateId(id) => write!(f, "hook id already registered: {id}"),
            RegisterError::UnknownId(id) => write!(f, "hook id not registered: {id}"),
            RegisterError::UnknownParam(key) => write!(f, "hook param not declared: {key}"),
            RegisterError::InvalidParam(msg) => write!(f, "invalid hook param: {msg}"),
        }
    }
}

impl std::error::Error for RegisterError {}

/// 注册条目：定义 + 开启状态 + 周期取值。
struct RegisteredHook {
    def: Arc<HookDef>,
    enabled: bool,
    /// 生效取值（初始 = 各 spec 的 `default`）。
    values: BTreeMap<&'static str, CycleValue>,
}

/// 分发快照项（锁外使用）：定义 + 取值副本。
struct ArmedHook {
    def: Arc<HookDef>,
    values: BTreeMap<&'static str, CycleValue>,
}

/// 可调项形态（出参视图）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HookParamKindView {
    Enum { values: Vec<String>, multi: bool },
    Bool,
    Number { min: i64, max: i64 },
}

/// 可调项出参（声明 + 当前取值）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookParamView {
    pub key: String,
    pub label: String,
    /// `"call_gate"` = 参与调用前判定；`"internal"` = 动作内部读取。
    pub usage: String,
    pub kind: HookParamKindView,
    #[serde(rename = "default")]
    pub default: CycleValue,
    pub value: CycleValue,
}

/// 清单出参：**完全由 `HookDef` + 生效值派生**（管理面不感知具体动作）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookEntry {
    pub id: String,
    pub label: String,
    pub group: String,
    pub inject_point: String,
    pub enabled: bool,
    /// 关停风险提示（i18n key）；`null` = 无提示。不阻止关停。
    pub disable_hint: Option<String>,
    pub params: Vec<HookParamView>,
}

/// 注入点注册表：按注入点分组、组内按注册顺序执行；`&mut` 直接链式传值。
///
/// 分发前统一做周期判定（`enabled` + `CallGate` 项），未命中记 skip 并跳过。
///
/// 内部 `Mutex`：注册 / 开关 / 取值 / 执行均为 `&self`（runner 以 `Arc<HookRegistry>` 共享，
/// 装配期与执行期并发访问；run_* 内部快照后锁外 await，不跨 await 持锁）。
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

    /// 注册（**默认关闭**）：同 id 重复 → `Err(DuplicateId)`；成功挂到
    /// `def.inject_point` 组内末尾（执行顺序）。开启须显式 [`Self::set_enabled`]；
    /// 周期取值初始化为各 `CycleParamSpec.default`。
    pub fn register(&self, def: HookDef) -> Result<(), RegisterError> {
        let mut hooks = self.inner.lock().expect("hook registry lock");
        if hooks.values().flatten().any(|h| h.def.id == def.id) {
            return Err(RegisterError::DuplicateId(def.id.to_string()));
        }
        let values = def
            .cycle_params
            .iter()
            .map(|spec| (spec.key, spec.default.clone()))
            .collect();
        hooks
            .entry(def.inject_point)
            .or_default()
            .push(RegisteredHook {
                def: Arc::new(def),
                enabled: false,
                values,
            });
        Ok(())
    }

    /// 开启 / 关闭已注册条目（装配期或运行期均可调）；目标 id 不存在 → `Err(UnknownId)`。
    ///
    /// **无硬保护**：任何动作都可关闭（`disable_hint` 仅面板提示）。
    pub fn set_enabled(&self, id: &str, on: bool) -> Result<(), RegisterError> {
        let mut hooks = self.inner.lock().expect("hook registry lock");
        for hook in hooks.values_mut().flatten() {
            if hook.def.id == id {
                hook.enabled = on;
                return Ok(());
            }
        }
        Err(RegisterError::UnknownId(id.to_string()))
    }

    /// 条目是否已开启（未注册视为未开启）。
    pub fn is_enabled(&self, id: &str) -> bool {
        let hooks = self.inner.lock().expect("hook registry lock");
        hooks
            .values()
            .flatten()
            .any(|h| h.def.id == id && h.enabled)
    }

    /// 条目是否已注册（与是否开启无关）。
    pub fn is_registered(&self, id: &str) -> bool {
        let hooks = self.inner.lock().expect("hook registry lock");
        hooks.values().flatten().any(|h| h.def.id == id)
    }

    /// 动作内部读取可调项（`usage = Internal`；`CallGate` 项同样可读）。
    pub fn param_of(&self, id: &str, key: &str) -> Option<CycleValue> {
        let hooks = self.inner.lock().expect("hook registry lock");
        hooks
            .values()
            .flatten()
            .find(|h| h.def.id == id)
            .and_then(|h| h.values.get(key).cloned())
    }

    /// 设置可调项取值：按该动作声明的 spec 校验（形态 / 候选值 / 范围）。
    pub fn set_value(&self, id: &str, key: &str, value: CycleValue) -> Result<(), RegisterError> {
        let mut hooks = self.inner.lock().expect("hook registry lock");
        let hook = hooks
            .values_mut()
            .flatten()
            .find(|h| h.def.id == id)
            .ok_or_else(|| RegisterError::UnknownId(id.to_string()))?;
        let spec = hook
            .def
            .cycle_params
            .iter()
            .find(|s| s.key == key)
            .ok_or_else(|| RegisterError::UnknownParam(format!("{id}.{key}")))?;
        validate_value(spec, &value).map_err(RegisterError::InvalidParam)?;
        hook.values.insert(spec.key, value);
        Ok(())
    }

    /// 清单：全部注册条目（**按 id 排序**，输出稳定），含 enabled / 分组 / 关停提示 / 可调项。
    pub fn snapshot_all(&self) -> Vec<HookEntry> {
        let hooks = self.inner.lock().expect("hook registry lock");
        let mut entries: Vec<HookEntry> = hooks
            .values()
            .flatten()
            .map(|h| HookEntry {
                id: h.def.id.to_string(),
                label: h.def.label.to_string(),
                group: h.def.group.to_string(),
                inject_point: h.def.inject_point.as_str().to_string(),
                enabled: h.enabled,
                disable_hint: h.def.disable_hint.map(|s| s.to_string()),
                params: h
                    .def
                    .cycle_params
                    .iter()
                    .map(|spec| HookParamView {
                        key: spec.key.to_string(),
                        label: spec.label.to_string(),
                        usage: match spec.usage {
                            CycleParamUsage::CallGate => "call_gate".to_string(),
                            CycleParamUsage::Internal => "internal".to_string(),
                        },
                        kind: match &spec.kind {
                            CycleParamKind::Enum { values, multi } => HookParamKindView::Enum {
                                values: values.iter().map(|v| v.to_string()).collect(),
                                multi: *multi,
                            },
                            CycleParamKind::Bool => HookParamKindView::Bool,
                            CycleParamKind::Number { min, max } => HookParamKindView::Number {
                                min: *min,
                                max: *max,
                            },
                        },
                        default: spec.default.clone(),
                        value: h
                            .values
                            .get(spec.key)
                            .cloned()
                            .unwrap_or_else(|| spec.default.clone()),
                    })
                    .collect(),
            })
            .collect();
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        entries
    }

    /// 组快照（**只取已开启条目**）：锁内取 Arc + 取值副本，锁外 await。
    fn snapshot(&self, point: InjectPointId) -> Vec<ArmedHook> {
        let hooks = self.inner.lock().expect("hook registry lock");
        hooks
            .get(&point)
            .map(|group| {
                group
                    .iter()
                    .filter(|h| h.enabled)
                    .map(|h| ArmedHook {
                        def: Arc::clone(&h.def),
                        values: h.values.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 周期判定：未命中记 skip 日志并返回 `false`（调用方跳过该 hook）。
    fn cycle_allows(def: &HookDef, values: &BTreeMap<&'static str, CycleValue>, ctx: &RoundContext) -> bool {
        let facts = CycleFacts::derive(ctx);
        match gate_check(def.cycle_params, values, &facts) {
            None => true,
            Some(key) => {
                tracing::info!(
                    phase = PHASE_HOOK_CYCLE_GATE,
                    hook_id = def.id,
                    inject_point = def.inject_point.as_str(),
                    param = key,
                    "hook skipped: cycle gate not matched"
                );
                false
            }
        }
    }

    /// IP-1：load_context 后。**fail 策略**——最早点未落库，Err 上抛中止本轮。
    ///
    /// 会话切换响应：hook 可改写 `ctx.session_id`（路由决策，如 assistant 课题切换会话）；
    /// 每次 hook 执行后若 session 变化，调用 `on_session_switch`（runner 传入 reload）重载
    /// 新会话上下文，后续 hooks 基于最终会话数据执行。
    pub async fn run_after_load_context(
        &self,
        ctx: &mut RoundContext,
        on_session_switch: impl Fn(&mut RoundContext) -> AppResult<()>,
    ) -> AppResult<()> {
        let hooks = self.snapshot(InjectPointId::AfterLoadContext);
        let mut last_session = ctx.session_id.clone();
        for hook in hooks {
            if !Self::cycle_allows(&hook.def, &hook.values, ctx) {
                continue;
            }
            if let HookHandler::AfterLoadContext(f) = &hook.def.handler {
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
        for hook in self.snapshot(InjectPointId::AfterPersistInput) {
            if !Self::cycle_allows(&hook.def, &hook.values, ctx) {
                continue;
            }
            if let HookHandler::AfterPersistInput(f) = &hook.def.handler {
                if let Err(e) = f(ctx).await {
                    tracing::warn!(
                        hook_id = hook.def.id,
                        error = %e,
                        "after_persist_input hook failed; sending original wire"
                    );
                }
            }
        }
    }

    /// IP-3：call_model 后、execute_tools 前。**ignore 策略**——Err 用原响应继续。
    pub async fn run_after_call_model(&self, ctx: &mut RoundContext, response: &mut ModelResponse) {
        for hook in self.snapshot(InjectPointId::AfterCallModel) {
            if !Self::cycle_allows(&hook.def, &hook.values, ctx) {
                continue;
            }
            if let HookHandler::AfterCallModel(f) = &hook.def.handler {
                if let Err(e) = f(ctx, response).await {
                    tracing::warn!(
                        hook_id = hook.def.id,
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
        results: &mut Vec<ToolResult>,
    ) {
        for hook in self.snapshot(InjectPointId::AfterExecuteTools) {
            if !Self::cycle_allows(&hook.def, &hook.values, ctx) {
                continue;
            }
            if let HookHandler::AfterExecuteTools(f) = &hook.def.handler {
                if let Err(e) = f(ctx, results).await {
                    tracing::warn!(
                        hook_id = hook.def.id,
                        error = %e,
                        "after_execute_tools hook failed; using original tool results"
                    );
                }
            }
        }
    }

    /// IP-5：persist_outcome 后。**ignore 策略**——产物已入库，Err 不影响本轮。
    pub async fn run_after_persist_outcome(&self, ctx: &RoundContext) {
        for hook in self.snapshot(InjectPointId::AfterPersistOutcome) {
            if !Self::cycle_allows(&hook.def, &hook.values, ctx) {
                continue;
            }
            if let HookHandler::AfterPersistOutcome(f) = &hook.def.handler {
                if let Err(e) = f(ctx).await {
                    tracing::warn!(
                        hook_id = hook.def.id,
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
    use crate::core::hook::cycle::{CycleField, RoundRef};
    use crate::core::models::{ChatModelSelection, ConversationMode, MessageBody, MessageRole};
    use crate::core::round_service::RoundTriggerKind;
    use crate::core::round_types::SessionState;

    fn ok() -> BoxFuture<'static, AppResult<()>> {
        Box::pin(async { Ok(()) })
    }

    fn boom() -> BoxFuture<'static, AppResult<()>> {
        Box::pin(async { Err(crate::core::error::AppError::RuntimeError("boom".into())) })
    }

    fn message(role: MessageRole) -> crate::core::models::Message {
        crate::core::models::Message {
            role,
            body: MessageBody::Text {
                content: String::new(),
                reasoning: None,
                tool_calls: None,
            },
            timestamp: 0,
            neuron_id: None,
            elapsed_ms: None,
        }
    }

    fn ctx(seed: Option<&str>) -> RoundContext {
        RoundContext {
            session_id: "s-1".into(),
            mode: ConversationMode::Chat,
            seed: seed.map(|s| crate::core::round_types::SessionSeed::Neuron(s.into())),
            state: SessionState::default(),
            messages: Vec::new(),
            model_input: String::new(),
            model: ChatModelSelection::new("test-provider", "test-model"),
            tool_override: None,
            trigger: RoundTriggerKind::User,
            topic_id: None,
            reselect: true,
            nudge_persist: false,
            selected_neuron: None,
            outcome: None,
        }
    }

    /// 测试用最小 HookDef（无周期可调项）。
    fn def(
        id: &'static str,
        label: &'static str,
        inject_point: InjectPointId,
        handler: HookHandler,
    ) -> HookDef {
        HookDef {
            id,
            label,
            inject_point,
            handler,
            group: "test.group",
            disable_hint: None,
            cycle_params: &[],
        }
    }

    fn sample_response() -> ModelResponse {
        ModelResponse {
            provider_id: "p".into(),
            model_id: "m".into(),
            output: "hello".into(),
            tool_calls: None,
            finish_reason: "stop".into(),
            reasoning: None,
        }
    }

    /// 计数型可调项：每 N 个用户轮次触发（默认 N=3）。
    fn every_n_param() -> CycleParamSpec {
        CycleParamSpec {
            key: "every",
            label: "test.every",
            field: Some(CycleField::UserRounds),
            round_ref: RoundRef::Current,
            kind: CycleParamKind::Number { min: 1, max: 50 },
            default: CycleValue::Int(3),
            usage: CycleParamUsage::CallGate,
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
                .register(def(
                    id,
                    tag,
                    InjectPointId::AfterPersistInput,
                    HookHandler::AfterPersistInput(Box::new(move |_ctx| {
                        let calls = std::sync::Arc::clone(&calls);
                        let order = std::sync::Arc::clone(&order);
                        Box::pin(async move {
                            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            order.lock().unwrap().push(tag.to_string());
                            Ok(())
                        })
                    })),
                ))
                .unwrap();
            registry.set_enabled(id, true).unwrap();
        }
        let mut c = ctx(None);
        registry.run_after_persist_input(&mut c).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(*order.lock().unwrap(), ["first", "second"]);
    }

    #[test]
    fn duplicate_id_rejected() {
        let registry = HookRegistry::new();
        let make = || {
            def(
                "dup",
                "dup",
                InjectPointId::AfterLoadContext,
                HookHandler::AfterLoadContext(Box::new(|_| ok())),
            )
        };
        assert!(registry.register(make()).is_ok());
        assert_eq!(
            registry.register(make()),
            Err(RegisterError::DuplicateId("dup".into()))
        );
    }

    #[tokio::test]
    async fn load_context_failure_propagates() {
        let registry = HookRegistry::new();
        registry
            .register(def(
                "fail",
                "fail",
                InjectPointId::AfterLoadContext,
                HookHandler::AfterLoadContext(Box::new(|_| boom())),
            ))
            .unwrap();
        registry.set_enabled("fail", true).unwrap();
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
            .register(def(
                "p",
                "p",
                InjectPointId::AfterPersistInput,
                HookHandler::AfterPersistInput(Box::new(|_| boom())),
            ))
            .unwrap();
        registry.set_enabled("p", true).unwrap();
        let mut c = ctx(None);
        registry.run_after_persist_input(&mut c).await; // 不 panic、不返回 Err
        assert_eq!(c.session_id, "s-1");
    }

    #[tokio::test]
    async fn call_model_hook_can_rewrite_response() {
        let registry = HookRegistry::new();
        registry
            .register(def(
                "rewrite",
                "rewrite",
                InjectPointId::AfterCallModel,
                HookHandler::AfterCallModel(Box::new(|_ctx, resp| {
                    Box::pin(async move {
                        resp.output = "rewritten".into();
                        Ok(())
                    })
                })),
            ))
            .unwrap();
        registry.set_enabled("rewrite", true).unwrap();
        let mut c = ctx(None);
        let mut resp = sample_response();
        registry.run_after_call_model(&mut c, &mut resp).await;
        assert_eq!(resp.output, "rewritten");
    }

    #[tokio::test]
    async fn execute_tools_hook_can_drop_results() {
        let registry = HookRegistry::new();
        registry
            .register(def(
                "drop",
                "drop",
                InjectPointId::AfterExecuteTools,
                HookHandler::AfterExecuteTools(Box::new(|_ctx, results| {
                    Box::pin(async move {
                        results.clear();
                        Ok(())
                    })
                })),
            ))
            .unwrap();
        registry.set_enabled("drop", true).unwrap();
        let mut c = ctx(None);
        let mut results = vec![ToolResult {
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
            .register(def(
                "audit",
                "audit",
                InjectPointId::AfterPersistOutcome,
                HookHandler::AfterPersistOutcome(Box::new(move |ctx| {
                    let seen2 = std::sync::Arc::clone(&seen2);
                    Box::pin(async move {
                        *seen2.lock().unwrap() = Some(ctx.session_id.clone());
                        Ok(())
                    })
                })),
            ))
            .unwrap();
        registry.set_enabled("audit", true).unwrap();
        let c = ctx(Some("n-1"));
        registry.run_after_persist_outcome(&c).await;
        assert_eq!(seen.lock().unwrap().as_deref(), Some("s-1"));
    }

    #[tokio::test]
    async fn empty_registry_is_noop() {
        let registry = HookRegistry::new();
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
        registry
            .register(def(
                "route",
                "route",
                InjectPointId::AfterLoadContext,
                HookHandler::AfterLoadContext(Box::new(|ctx| {
                    let target = ctx.session_id.clone() + "-b";
                    Box::pin(async move {
                        ctx.session_id = target;
                        Ok(())
                    })
                })),
            ))
            .unwrap();
        registry.set_enabled("route", true).unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let seen2 = std::sync::Arc::clone(&seen);
        registry
            .register(def(
                "selection",
                "selection",
                InjectPointId::AfterLoadContext,
                HookHandler::AfterLoadContext(Box::new(move |ctx| {
                    let seen2 = std::sync::Arc::clone(&seen2);
                    Box::pin(async move {
                        *seen2.lock().unwrap() = ctx.session_id.clone();
                        Ok(())
                    })
                })),
            ))
            .unwrap();
        registry.set_enabled("selection", true).unwrap();
        let mut c = ctx(None);
        let reload_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let reload_calls2 = std::sync::Arc::clone(&reload_calls);
        registry
            .run_after_load_context(&mut c, |ctx| {
                reload_calls2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
        assert_eq!(seen.lock().unwrap().as_str(), "s-1-b", "hook2 应看到 reload 后的最终会话");
        assert!(matches!(
            c.seed,
            Some(crate::core::round_types::SessionSeed::Neuron(ref n)) if n == "reloaded"
        ));
    }

    #[tokio::test]
    async fn registered_but_disabled_does_not_run() {
        let registry = HookRegistry::new();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls2 = std::sync::Arc::clone(&calls);
        registry
            .register(def(
                "off",
                "off",
                InjectPointId::AfterPersistInput,
                HookHandler::AfterPersistInput(Box::new(move |_ctx| {
                    let calls = std::sync::Arc::clone(&calls2);
                    Box::pin(async move {
                        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        Ok(())
                    })
                })),
            ))
            .unwrap();
        assert!(registry.is_registered("off"));
        assert!(!registry.is_enabled("off"));
        let mut c = ctx(None);
        registry.run_after_persist_input(&mut c).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn set_enabled_toggles_runtime() {
        let registry = HookRegistry::new();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls2 = std::sync::Arc::clone(&calls);
        registry
            .register(def(
                "toggle",
                "toggle",
                InjectPointId::AfterPersistInput,
                HookHandler::AfterPersistInput(Box::new(move |_ctx| {
                    let calls = std::sync::Arc::clone(&calls2);
                    Box::pin(async move {
                        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        Ok(())
                    })
                })),
            ))
            .unwrap();
        let mut c = ctx(None);
        registry.set_enabled("toggle", true).unwrap();
        registry.run_after_persist_input(&mut c).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        registry.set_enabled("toggle", false).unwrap();
        registry.run_after_persist_input(&mut c).await;
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "关闭后不再分发"
        );
        assert!(registry.is_registered("toggle"), "关闭不等于注销");
    }

    #[test]
    fn set_enabled_unknown_id_errors() {
        let registry = HookRegistry::new();
        assert_eq!(
            registry.set_enabled("nope", true),
            Err(RegisterError::UnknownId("nope".into()))
        );
        assert!(!registry.is_enabled("nope"));
        assert!(!registry.is_registered("nope"));
    }

    fn gate_params() -> &'static [CycleParamSpec] {
        // `&'static [CycleParamSpec]` 需常量；用 Box::leak 在测试中构造一次。
        static ONCE: std::sync::OnceLock<&'static [CycleParamSpec]> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| Box::leak(vec![every_n_param()].into_boxed_slice()))
    }

    #[tokio::test]
    async fn cycle_gate_every_n_users_controls_dispatch() {
        let registry = HookRegistry::new();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls2 = std::sync::Arc::clone(&calls);
        registry
            .register(HookDef {
                id: "every3",
                label: "every3",
                inject_point: InjectPointId::AfterPersistInput,
                handler: HookHandler::AfterPersistInput(Box::new(move |_ctx| {
                    let calls = std::sync::Arc::clone(&calls2);
                    Box::pin(async move {
                        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        Ok(())
                    })
                })),
                group: "test.group",
                disable_hint: None,
                cycle_params: gate_params(),
            })
            .unwrap();
        registry.set_enabled("every3", true).unwrap();

        // 0 条用户消息 → 0 % 3 == 0 → 放行
        let mut c = ctx(None);
        registry.run_after_persist_input(&mut c).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);

        // 1 条用户消息 → 不匹配 → 跳过
        let mut c = ctx(None);
        c.messages.push(message(MessageRole::User));
        registry.run_after_persist_input(&mut c).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1, "门控未命中不执行");

        // 3 条用户消息 → 放行
        let mut c = ctx(None);
        for _ in 0..3 {
            c.messages.push(message(MessageRole::User));
        }
        registry.run_after_persist_input(&mut c).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);

        // 覆盖取值为 N=1 → 每轮都放行
        registry.set_value("every3", "every", CycleValue::Int(1)).unwrap();
        let mut c = ctx(None);
        c.messages.push(message(MessageRole::User));
        registry.run_after_persist_input(&mut c).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn set_value_validates_and_param_of_reads() {
        let registry = HookRegistry::new();
        registry
            .register(HookDef {
                id: "withparam",
                label: "withparam",
                inject_point: InjectPointId::AfterPersistInput,
                handler: HookHandler::AfterPersistInput(Box::new(|_| ok())),
                group: "test.group",
                disable_hint: None,
                cycle_params: gate_params(),
            })
            .unwrap();
        assert_eq!(registry.param_of("withparam", "every"), Some(CycleValue::Int(3)));
        assert!(registry.set_value("withparam", "every", CycleValue::Int(10)).is_ok());
        assert_eq!(registry.param_of("withparam", "every"), Some(CycleValue::Int(10)));
        // 越界
        assert!(matches!(
            registry.set_value("withparam", "every", CycleValue::Int(999)),
            Err(RegisterError::InvalidParam(_))
        ));
        // 形态不符
        assert!(matches!(
            registry.set_value("withparam", "every", CycleValue::Bool(true)),
            Err(RegisterError::InvalidParam(_))
        ));
        // 未声明 key
        assert!(matches!(
            registry.set_value("withparam", "nope", CycleValue::Int(1)),
            Err(RegisterError::UnknownParam(_))
        ));
        // 未注册 id
        assert!(matches!(
            registry.set_value("ghost", "every", CycleValue::Int(1)),
            Err(RegisterError::UnknownId(_))
        ));
    }

    #[test]
    fn snapshot_all_is_spec_derived_and_stable() {
        let registry = HookRegistry::new();
        registry
            .register(HookDef {
                id: "b-hook",
                label: "b.label",
                inject_point: InjectPointId::AfterPersistInput,
                handler: HookHandler::AfterPersistInput(Box::new(|_| ok())),
                group: "test.group",
                disable_hint: Some("test.hint"),
                cycle_params: gate_params(),
            })
            .unwrap();
        registry
            .register(def(
                "a-hook",
                "a.label",
                InjectPointId::AfterLoadContext,
                HookHandler::AfterLoadContext(Box::new(|_| ok())),
            ))
            .unwrap();
        let entries = registry.snapshot_all();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "a-hook", "按 id 排序，输出稳定");
        assert_eq!(entries[1].id, "b-hook");
        assert_eq!(entries[1].group, "test.group");
        assert_eq!(entries[1].disable_hint.as_deref(), Some("test.hint"));
        assert_eq!(entries[1].inject_point, "after_persist_input");
        assert_eq!(entries[1].enabled, false, "注册默认关闭");
        let param = &entries[1].params[0];
        assert_eq!(param.key, "every");
        assert_eq!(param.usage, "call_gate");
        assert_eq!(param.default, CycleValue::Int(3));
        assert_eq!(param.value, CycleValue::Int(3));
        assert!(matches!(param.kind, HookParamKindView::Number { min: 1, max: 50 }));
        assert!(entries[0].params.is_empty());
    }

    #[test]
    fn any_hook_can_be_disabled_even_with_hint() {
        let registry = HookRegistry::new();
        registry
            .register(HookDef {
                id: "hinted",
                label: "hinted",
                inject_point: InjectPointId::AfterPersistInput,
                handler: HookHandler::AfterPersistInput(Box::new(|_| ok())),
                group: "test.group",
                disable_hint: Some("test.hint"),
                cycle_params: &[],
            })
            .unwrap();
        assert!(
            registry.set_enabled("hinted", false).is_ok(),
            "disable_hint 不阻止关停（无硬保护）"
        );
        assert!(!registry.is_enabled("hinted"));
    }
}

# Plan: 思考模式强度管理（reasoning effort）落地技术方案

## Goal

把现有「思考模式开关 + 强度」实现对齐 OpenAI 官方协议规范，收敛三处缺口：

1. `reasoning_effort` 从 `extra` 透传提升为协议层一等字段（类型安全 + 分层正确）。
2. `ThinkingEffort` 由 3 档扩至官方 7 档，并前向兼容未知档位。
3. `ThinkingCapability` 增加「支持档位白名单」，`resolve_thinking` 据此钳制非法值。

**非目标（Out of Scope）**：Responses API 对接（仅预留结构）、思考过程 UI 展示改造、思考 token 计费 UI。

---

## 现状核实（代码事实）

| 位置 | 现状 | 问题 |
|---|---|---|
| `providers/providers.rs:1152` `apply_thinking` | `req.extra.insert("reasoning_effort", …)` 透传 | 官方标准字段被当服务商扩展，破坏「协议层不含服务商策略」边界 |
| `core/models.rs:349` `ThinkingEffort` | `Low / High / Max` 三档 | 与官方 7 档（`none/minimal/low/medium/high/xhigh/max`）不符 |
| `core/models.rs:377` `ThinkingCapability` | `supported` / `default_enabled` / `default_effort` | 无「哪几档可用」声明；官方明言「Not all reasoning models support every value」 |
| `providers/providers.rs:1032` `resolve_thinking` | 调用级 > 模型默认；不支持则 `None` | 未校验 `effort` 合法性，越权值会透传到服务端 |
| `core/models.rs` `SamplingParams.max_tokens` | 单字段 | 推理模型须用 `max_completion_tokens`（含 reasoning tokens），本期不改但需记录风险 |
| `providers/openai_compat.rs:594` | 已有 `extra` 注入 `reasoning_effort` 的单测 | 需随字段提升改写 |

**协议依据**（来自本课题调研）：`openai-python` `shared/reasoning_effort.py` 取值集；官方字段说明原文「Currently supported values are `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, and `max` … Not all reasoning models support every value.」

---

## 改动点清单

### P0 — `reasoning_effort` 提升为协议层标准字段

**文件**：`providers/openai_compat.rs`、`providers/providers.rs`

1. `ChatRequest` 增加显式字段（与 `temperature` 等采样字段同级）：
   ```rust
   /// OpenAI 官方标准：思考强度。服务商差异由 providers 层抹平后写入。
   #[serde(skip_serializing_if = "Option::is_none")]
   pub reasoning_effort: Option<String>,
   ```
2. `apply_thinking` 改写：`reasoning_effort` 写字段；DeepSeek `thinking` 开关**保留在 `extra`**：
   ```rust
   fn apply_thinking(req: &mut ChatRequest, thinking: Option<&ThinkingConfig>) {
       let Some(th) = thinking else { return };
       if let Some(effort) = th.effort {
           req.reasoning_effort = Some(thinking_effort_wire(effort).to_string());
       }
       if let Some(enabled) = th.enabled {
           // DeepSeek 等特异扩展仍走 extra 透传（协议层不含服务商策略）
           req.extra.insert(
               "thinking".to_string(),
               serde_json::json!({ "type": if enabled { "enabled" } else { "disabled" } }),
           );
       }
   }
   ```

**影响范围**：`openai_compat.rs`（类型 + 序列化）、`providers.rs`（注入逻辑）、既有单测。
**风险**：低。序列化后 JSON 形态不变（仍在顶层），wire 无破坏；`extra` 中 `reasoning_effort` 残留需清理以防重复。**注意**：`extra` 为 `#[serde(flatten)]`，若同名键既在字段又在 `extra`，需确认无冲突。

### P1 — `ThinkingEffort` 扩至 7 档 + 前向兼容

**文件**：`core/models.rs`、`providers/providers.rs`

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    /// 前向兼容：官方新增档位时不 panic（透传即忽略）。
    #[serde(other)]
    Unknown,
}
```

`thinking_effort_wire` 同步扩展；`Unknown` 返回 `None`（不写入 wire）：
```rust
fn thinking_effort_wire(effort: ThinkingEffort) -> Option<&'static str> {
    match effort {
        ThinkingEffort::None => Some("none"),
        ThinkingEffort::Minimal => Some("minimal"),
        ThinkingEffort::Low => Some("low"),
        ThinkingEffort::Medium => Some("medium"),
        ThinkingEffort::High => Some("high"),
        ThinkingEffort::Xhigh => Some("xhigh"),
        ThinkingEffort::Max => Some("max"),
        ThinkingEffort::Unknown => None,
    }
}
```

**影响范围**：`models.rs`、`providers.rs`、`lib.rs`/`tui`/`cli`（若为 `ThinkingEffort` 构造处，需补匹配臂）、前端 `types.ts`、ModelPicker UI。
**风险**：中。Rust `match` 穷尽性会因新增变体产生编译错误，**这是好事**（编译器帮你找全改点）；`#[serde(other)]` 需 `Deserialize` 时以字符串反序列化为前提（当前 `rename_all="lowercase"` 满足）。
**兼容性**：旧配置仅含 `low/high/max` → 正常反序列化；`none` 与 `enabled=false` 语义重叠，见「决策点」。

### P1 — `ThinkingCapability.allowed_efforts` 白名单钳制

**文件**：`core/models.rs`、`providers/providers.rs`

```rust
pub struct ThinkingCapability {
    pub supported: bool,
    pub default_enabled: Option<bool>,
    pub default_effort: Option<ThinkingEffort>,
    /// 模型支持的档位白名单；None = 不限制（回退到全局 7 档）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_efforts: Option<Vec<ThinkingEffort>>,
}
```

`resolve_thinking` 增加钳制（放在默认值填充之后、`enabled==false` 清理之前）：
```rust
// 白名单钳制：越权档位回落到模型默认，仍非法则丢弃（providers 抹平）。
if let Some(allowed) = model_cap.and_then(|m| m.allowed_efforts.as_ref()) {
    let ok = out.effort.map(|e| allowed.contains(&e)).unwrap_or(true);
    if !ok {
        out.effort = model_cap.and_then(|m| m.default_effort)
            .filter(|e| allowed.contains(e));
    }
}
```

**影响范围**：`models.rs`、`providers.rs`、模型定义配置（config.json schema）、ProviderManager 模型编辑 UI。
**风险**：中。若旧模型定义未声明 `allowed_efforts` → `None` → 不限制，向后兼容；需防「default_effort 不在白名单」的配置错误（上述代码已 filter）。

### P2 — 预留 Responses 嵌套结构 + 记录 max_tokens 风险

- `reasoning` 嵌套对象（`{effort, summary}`）本期**不实现**，但建议在 `ChatRequest` 注释标注「Responses 为嵌套形态，接入时由 provider kind 决定展平/嵌套」。
- `SamplingParams` 增加 `max_completion_tokens` 字段列为**后续迭代**（推理模型必须，且含 reasoning tokens）。

---

## 测试用例

| # | 层级 | 用例 | 期望 |
|---|---|---|---|
| 1 | `openai_compat` 单测 | `reasoning_effort=high` 经 `ChatRequest` 序列化 | JSON 顶层 `reasoning_effort == "high"`，且不出现在其他位置 |
| 2 | `openai_compat` 单测 | DeepSeek `thinking` 开关 | 仍在 `extra` 展平为顶层 `thinking.type` |
| 3 | `models` 单测 | 反序列化 7 档全部取值 | 均成功且大小写不敏感（lowercase） |
| 4 | `models` 单测 | 反序列化未知档位 `"turbo"` | → `ThinkingEffort::Unknown`，不 panic |
| 5 | `providers` 单测 | `allowed_efforts=[low,high]`，调用传 `max` | `resolve_thinking` 回落 `default_effort` |
| 6 | `providers` 单测 | `supported=false` + 调用传 effort | 返回 `None`（沿用现有行为） |
| 7 | `providers` 单测 | `enabled=Some(false)` + effort | `effort=None`（互斥，沿用现有行为） |
| 8 | `providers` 单测 | 模型未声明 `allowed_efforts` | 任意合法档位透传（不钳制） |
| 9 | 前端 | `pnpm check` | 0 error；ModelPicker 按 `allowed_efforts` 渲染 |

**回归**：`cargo test --lib` 全绿（排除既有 Windows flaky `execute_timeout_kills_process`）；`pnpm check`。

---

## 分步落地顺序

| 步 | 内容 | 验收 | 可独立合入 |
|---|---|---|---|
| 1 | P0：`ChatRequest.reasoning_effort` + `apply_thinking` 改写 + 用例 1/2 | 单测绿 | ✅ |
| 2 | P1：`ThinkingEffort` 扩 7 档 + `Unknown` + 用例 3/4 | 编译通过 + 单测绿 | ✅ |
| 3 | P1：`allowed_efforts` + `resolve_thinking` 钳制 + 用例 5/6/7/8 | 单测绿 | ✅ |
| 4 | 前端：`types.ts` 同步 + ModelPicker 按白名单渲染 + 用例 9 | `pnpm check` 0 error | ✅ |
| 5 | （后续）Responses 嵌套 + `max_completion_tokens` | 另起迭代 | — |

**每步均向后兼容**（新增字段皆 `Option` + `skip_serializing_if`；枚举新增变体不破坏旧数据）。

---

## 风险与权衡

| 风险 | 等级 | 说明 | 缓解 |
|---|---|---|---|
| `extra` 与显式字段同键冲突 | 中 | `#[serde(flatten)]` 下重复键行为未验证 | 步 1 前先写探针单测确认；或序列化前从 `extra` 移除同名键 |
| 枚举扩展引发大面积编译错误 | 中 | `match` 穷尽性 | 视为正向收益，编译器定位全部改点 |
| `none` 与 `enabled=false` 语义重叠 | 中 | 两个入口表达「关闭」 | 见决策点，需定优先级 |
| 旧模型定义缺 `allowed_efforts` | 低 | 旧配置 | `None` = 不限制，天然兼容 |
| 前端与后端枚举不同步 | 低 | 手写 `types.ts` | 步 4 一次性同步 + 用例 9 |

---

## 决策点（需用户确认）

1. **`none` 与 `enabled=false` 的优先级**：建议「`enabled==false` 优先，强制清空 effort」；若用户显式传 `effort=none` 而 `enabled` 未指定，则视为关闭。
2. **是否本期就对齐 7 档**：若目标服务商（如 DeepSeek）仅接受 `low/high/max`，扩档后需在 `allowed_efforts` 层收紧，避免透传非法值。
3. **Responses 是否纳入近期规划**：影响是否现在就把内部抽象做成 `reasoning` 对象（一次性设计 vs 二次重构）。

---

## Scope

- **In**：`providers/openai_compat.rs`、`providers/providers.rs`、`core/models.rs`、前端 `types.ts` + ModelPicker、本迭代测试。
- **Out**：Responses API 对接、`max_completion_tokens`、思考过程 UI 展示、思考 token 计费展示。

## Change Log

- 2026-09-19: 初版（基于 OpenAI reasoning effort 调研结论 + 仓库现状核实，输出 P0-P2 改动点、测试用例、分步顺序与决策点）

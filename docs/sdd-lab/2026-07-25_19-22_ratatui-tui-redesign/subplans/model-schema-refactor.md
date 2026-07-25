# Model Schema Refactor 技术方案

## 1. 动机

当前的 `ModelCapabilities` 只有 `{ chat, tools, streaming }` 三个布尔字段，但主流模型 API（OpenAI、DeepSeek 等）实际提供的信息要丰富得多：上下文窗口、最大输出、定价、知识截止日期、视觉支持等。

不同厂商使用的字段名和能力集不一致，需要一个可伸缩的公共 schema 来统一表达。

## 2. 设计原则

1. **公共字段标准化** — 同义字段统一命名（如 `function_calling` / `tool_calls` 统一为 `supports_tools`）
2. **Optional 优先** — 不存在的字段用 `Option<T>`，`None` 表示该厂商未提供
3. **向后兼容** — 旧的 `capabilities: { chat, tools, streaming }` 配置仍然可读
4. **厂商特有字段进** **`extras`** — 难以统一到公共字段的能力（如 DeepSeek 的 thinking\_mode）放进 JSON object 兜底

## 3. 模型变更

### 3.1 当前（现状）

```rust
struct ModelCapabilities {
    chat: bool,
    tools: bool,
    streaming: bool,
}

struct ModelInfo {
    id: String,
    provider_id: String,
    display_name: String,
    capabilities: ModelCapabilities,
}
```

### 3.2 新结构

```rust
/// 公共标准化能力标记
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelCapabilities {
    // ---- 基础对话能力 ----
    pub chat: bool,
    pub tools: bool,
    pub streaming: bool,
    pub structured_output: bool,   // JSON mode / structured outputs

    // ---- 可选的多模态能力 ----
    pub vision: Option<bool>,
    pub audio: Option<bool>,

    // ---- 厂商特有 ----
    /// 无法标准化到公共字段的能力存放处
    /// 如 DeepSeek: { "thinking_mode": true }
    ///     OpenAI:  { "code_interpreter": true, "web_search": true }
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub provider_id: String,
    pub display_name: String,
    pub capabilities: ModelCapabilities,

    // ---- 容量 ----
    pub context_window: Option<u32>,        // e.g. 128000
    pub max_output_tokens: Option<u32>,     // e.g. 8192

    // ---- 定价 (per 1M tokens) ----
    pub pricing_input: Option<f64>,         // e.g. 2.50
    pub pricing_output: Option<f64>,        // e.g. 15.00
    pub pricing_cache_input: Option<f64>,   // DeepSeek 独有

    // ---- 知识 ----
    pub knowledge_cutoff: Option<String>,   // e.g. "2025-06"
}
```

### 3.3 反序列化兼容

新 `ModelCapabilities` 增加了非 optional 字段 `structured_output`，需要提供 `Default` 以使旧配置 `capabilities: { chat: true, tools: false, streaming: false }` 仍然能反序列化。

`structured_output` 的默认值：`false`。用户有需要时自行配置。

### 3.4 厂商独有能力示例配置

```json
# DeepSeek
{
  "id": "deepseek-v4-flash",
  "capabilities": {
    "chat": true,
    "tools": true,
    "streaming": true,
    "structured_output": true,
    "vision": false,
    "extras": {
      "thinking_mode": true,
      "fim_completion": true
    }
  },
  "context_window": 1000000,
  "max_output_tokens": 384000,
  "pricing_input": 0.14,
  "pricing_output": 0.28,
  "pricing_cache_input": 0.0028
}

# OpenAI
{
  "id": "gpt-5.5-pro",
  "capabilities": {
    "chat": true,
    "tools": true,
    "streaming": true,
    "structured_output": true,
    "vision": true,
    "extras": {
      "web_search": true,
      "code_interpreter": true,
      "image_generation": true
    }
  },
  "context_window": 1050000,
  "max_output_tokens": 128000,
  "pricing_input": 2.50,
  "pricing_output": 15.00,
  "knowledge_cutoff": "2025-08"
}
```

## 4. 代码变更清单

| 文件                       | 变更                                    |
| ------------------------ | ------------------------------------- |
| `core/models.rs`         | 扩展 `ModelCapabilities`，扩展 `ModelInfo` |
| `core/providers.rs`      | `ConfiguredModel` 结构体匹配新字段            |
| `bin/agent-app-cli.rs`   | `print_models()` 展示新字段                |
| `tui/commands.rs`        | `cmd_models_text()` 展示新字段             |
| `.agent-app/config.json` | 按新字段更新样例配置                            |

**不涉及**：gateway.rs、app.rs 的会话管理逻辑、event.rs、render.rs 的核心布局——这些都是纯数据层变更。

## 5. 显示策略

TUI 的 `/models <provider>` 输出按优先级分组展示：

```
  deepseek-v4-flash
    ├─ capacity: 1M ctx | 384K max output
    ├─ pricing : $0.14 in / $0.28 out
    ├─ features: chat ✓ tools ✓ streaming ✓ json ✓ vision —
    └─ extras  : thinking_mode ✓ fim_completion ✓
```

CLI 的 `print_models` 保持单行紧凑格式：

```
  deepseek-v4-flash | 1M ctx | $0.14/$0.28 | chat tools streaming json
```

## 6. 变更影响

* **无破坏性变更**：旧配置文件只需新增 `structured_output: false` 即可继续使用

* **TUI 重构进度不受影响**：数据层变更与渲染层解耦

* **预估工时**：纯代码变更约 30-40 行，配置更新约 15 行


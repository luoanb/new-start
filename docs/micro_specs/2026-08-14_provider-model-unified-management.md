# 服务商与模型：聚合面板 + 管理能力（方案）

- 日期：2026-08-14
- 状态：待评审
- 影响包：`packages/pulsar-app`（Svelte 前端 + Tauri/Rust 后端）

## 1. 背景与需求

当前界面中「服务商」（`ProvidersPanel`）与「模型」（`ModelsPanel`）是**两个独立的只读面板**，只能查看，无法在界面中管理。用户提出两点需求：

1. **聚合**：服务商与模型聚合在同一个 panel 内展示，避免两个独立面板割裂信息。
2. **可管理**：服务商与模型需要支持增删改（含 API Key、模型能力、定价等配置），保存即生效。

## 2. 现状分析

| 层 | 现状 |
| --- | --- |
| 前端视图 | [views.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/views.ts) 注册 `providers` / `models` 两个独立视图，均可拖拽到任意容器 |
| 前端面板 | [ProvidersPanel.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/ProvidersPanel.svelte) 与 [ModelsPanel.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/ModelsPanel.svelte) 均为纯展示，无任何写操作 |
| 前端数据 | [dataStore.svelte.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/stores/dataStore.svelte.ts) bootstrap 时 `list_providers` / `list_models` 一次性拉取，只读 |
| 后端注册表 | [providers.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/providers.rs) `ProviderRegistry`：**4 个内置服务商硬编码在代码**（openai / deepseek / ollama / custom），config.json 仅能覆盖 api_base / api_key / models |
| 后端命令 | `list_providers` / `list_models` 两个只读命令，无写命令 |
| 配置文件 | `.pulsar/config.json`：`defaults` + `poller` + `providers.<id>.{api_key, api_base, models[]}` |
| 可借鉴先例 | **ToolConfig**：`get_tool_config` → 弹窗编辑 → 校验 → 原子写回 JSON → 重装配（保存即生效）的完整链路；**NeuronManager**：列表 + main 区画布编辑器，`layoutStore.insertPanel()` 打开编辑器的模式 |

### 核心约束

- 内置服务商定义在 Rust 代码中（`default_providers()`），config.json 无法物理删除它们 → 删除内置 = 写入禁用标记。
- `ProviderRegistry` 是 Tauri `State` 单例，保存后需要**热重载**才能即时生效。
- `auth_env` 是 `&'static str`（如 `OPENAI_API_KEY`），自定义服务商没有对应环境变量，需要支持把 key 存 config.json 或指定自定义 env 名。
- 布局存储（`layoutStorage.ts`）已存在 v8→v9 迁移机制，合并视图需要新增一条布局迁移，兼容用户已保存的旧布局。

## 3. 已确认的关键决策

| 决策点 | 结论 |
| --- | --- |
| 内置服务商管理范围 | **全部可自由增删改**；删除内置 = 写入 `enabled: false` 禁用标记（代码定义无法物理删，重启后不复活） |
| API Key 策略 | **可配置 + 掩码回显**：可新增/覆盖写入 config.json，回显只显示掩码（`sk-****`），无法读回明文 |
| 面板形态 | **分组树形**：服务商为分组（可折叠），模型作为子项展开在组内 |
| 管理入口 | **main 区全屏编辑器**：侧栏面板只读展示 + 行内操作，点「管理/编辑」跳转 main 区打开全屏编辑器（对齐 ToolEditor / NeuronManager 模式） |

## 4. 后端设计

### 4.1 配置文件扩展（`.pulsar/config.json`）

```jsonc
{
  "defaults": { "provider": "deepseek", "model": "deepseek-v4-flash" },
  "providers": {
    "openai": {                       // 内置：仅覆盖字段，删除语义 = enabled:false
      "display_name": "OpenAI",
      "kind": "openai",               // 内置不可改；新增服务商必填
      "api_key": "sk-...",
      "api_base": "https://api.openai.com/v1",
      "auth_env": "OPENAI_API_KEY",   // 可选；自定义服务商指定 env 名或省略
      "enabled": true,                // false = 禁用隐藏（内置删除）
      "models": [ /* 同现状，模型 CRUD 均落在该数组 */ ]
    },
    "my-llm": {                       // 自定义：完整定义
      "display_name": "My LLM",
      "kind": "openai_compatible",
      "api_key": "...",
      "api_base": "https://...",
      "enabled": true,
      "models": []
    }
  }
}
```

### 4.2 `ProviderRegistry` 重构（providers.rs）

- `ProviderDefinition` 新增字段：`builtin: bool`、`enabled: bool`（内置默认 true）。
- 装配逻辑：`default_providers()`（内置，标记 builtin）+ 合并 config.json 自定义 provider；内置若被配置 `enabled: false` 则过滤掉。自定义 provider 不写 enabled 字段默认 true，删除自定义 = 从配置移除键。
- 新增方法：
  - `pub fn reload(&mut self) -> AppResult<()>`：重读 config.json 并重装配（保存后调用，即时生效）。
  - `pub fn get_config_view(&self) -> AppResult<ProviderConfigView>`：返回**可编辑完整视图**（含内置定义 + config 覆盖结果 + enabled / builtin 元信息；api_key 一律掩码回显）。
  - `pub fn save_config(&mut self, view: ProviderConfigView) -> AppResult<()>`：校验 → 合并写回 config.json（原子写：临时文件 + rename，对齐 ToolConfig）→ `reload()`。
- `ProviderConfigView` 即 config.json 形态的 serde 结构 + 编辑所需元信息，前端编辑器整体读写（对齐 ToolConfigView 先例，不拆细粒度 CRUD，保证原子性）。
- api_key 掩码处理：`save_config` 收到与掩码占位（`sk-****`）完全相同的值时视为「未修改」，保留原值。

### 4.3 新增 Tauri 命令（lib.rs）

| 命令 | 入参 | 说明 |
| --- | --- | --- |
| `get_provider_config` | - | 返回 `ProviderConfigView`（编辑器初始化） |
| `save_provider_config` | `view: ProviderConfigView` | 校验 → 写回 → reload，返回处理后的视图；校验失败拒绝保存并返回可读错误 |

保存成功后广播 `StateChange::Providers`（新增事件 kind），前端据此刷新。

### 4.4 校验规则

- provider id：非空、`[a-zA-Z0-9_-]`、不与现有 id 冲突（编辑时除外）。
- kind：新增服务商仅允许 `openai_compatible`；内置 kind 不可改。
- 每个 `models[].id` 非空。
- `defaults` 引用的 provider/model 必须存在于保存后的结果中（删除被引用项时拒绝保存或自动回退，采用**拒绝并提示**）。
- 校验失败不写盘，返回可读错误（对齐 `save_tool_config` 行为）。

## 5. 前端设计

### 5.1 视图注册与布局迁移（views.ts / layoutStorage.ts）

- 合并 `providers` + `models` 为单个视图 id `providers-models`（组件 `ProvidersModelsPanel`），删除两个旧注册。
- `layoutStorage.ts` 新增迁移（沿用 v8→v9 机制）：旧布局中出现 `providers` 或 `models` 的容器，**合并为一个 `providers-models`**（多个 panel 去重，取首个位置）。
- mainViews 新增 `provider-manager`（全屏编辑器），`activityItems` 不变（入口仍从侧栏面板进）。

### 5.2 聚合面板 `ProvidersModelsPanel.svelte`（侧栏，分组树形）

```
[+ 新增服务商]  [🔍 搜索]
▸ openai        OpenAI  · 内置  · 4 个模型   [编辑] [删除]
   ▸ gpt-5.6-sol    GPT-5.6 Sol  [chat][tools]  [编辑] [删除]
   ▸ ...
▾ deepseek     DeepSeek  · 3 个模型           [编辑] [删除]
   ▸ deepseek-v4-flash ...
```

- 每行服务商：名称 + id + kind/内置/禁用徽标 + 模型数 + 展开箭头 + 操作按钮。
- 展开后模型子项：名称 + id + 能力标签（复用现有 `modelCaps` 逻辑）+ 操作按钮。
- 操作按钮：编辑/删除；「编辑」或「新增」→ `dataStore.requestEditProvider(id)` / `requestCreateProvider()` → `layoutStore.insertPanel("provider-manager")` 打开 main 区编辑器（对齐 `requestEditNeuron` 模式，用 store 传递目标 id）。
- 删除需二次确认（内置提示「禁用后将从列表隐藏」）。
- 数据仍来自 `dataStore.state.providers / models`（bootstrap 全量拉取不变）。

### 5.3 main 区全屏编辑器 `ProviderManager.svelte`

- 布局：左侧服务商列表（含新建/选中高亮），右侧表单。
- 服务商表单：id（新建可填、编辑只读）、display_name、kind（内置只读下拉）、api_base、api_key（掩码 + 「修改后替换」语义）、auth_env（可选）、enabled（开关，内置删除同义）。
- 模型编辑：列表行编辑（id / display_name / 能力勾选 / context_window / max_output_tokens / pricing 输入），支持行内增删。
- 顶栏：保存 / 放弃（脏状态提示）。
- 保存 → `save_provider_config` → 成功刷新 `dataStore`（providers + models），并同步刷新「默认模型」引用提示。

### 5.4 dataStore / 事件扩展

- `StateEventKind` 与 `StateChangePayload` 增加 `providers`。
- 新增 action：`refreshProvidersModels()`；`requestEditProvider(id)` / `requestCreateProvider()`（消费后置 null，对齐神经元模式）。
- `ProviderManager` 通过 store 消费 `providerEditRequestId` / `providerCreateRequest` 打开对应编辑态。

### 5.5 i18n

- 新增 `views.providersModels`、`providerManager.*`（编辑器文案）、`sidePanel` 复用现有 key，补充 `builtin` / `disabled` / `apiKeyMasked` 等少量新词条（中英双语）。

## 6. 实施步骤（拆分）

1. **后端核心**：`ProviderConfigView` 结构 + `ProviderRegistry` 重构（builtin/enabled 装配、reload、get/save config、掩码处理）+ 校验函数。
2. **后端命令**：`get_provider_config` / `save_provider_config` + `StateChange::Providers` 事件；补充单测（内置禁用复活、掩码不覆盖、非法 id 拒绝、defaults 引用校验）。
3. **前端数据层**：dataStore action / 事件订阅 / 视图注册与布局迁移。
4. **聚合面板**：`ProvidersModelsPanel.svelte`（分组树形 + 行内操作 + 删除确认）。
5. **全屏编辑器**：`ProviderManager.svelte`（服务商表单 + 模型行编辑 + 保存/放弃）。
6. **收尾**：i18n 词条、视觉走查（对照 BitsUI）、手动验证（增删改内置/自定义服务商、模型 CRUD、掩码回显、保存即生效、重启不复活已删内置）。

## 7. 风险与边界

- **内置删除语义**：删除内置 = `enabled: false`，config.json 中仍保留其节点；已删内置若被会话引用会得到明确的不可用提示。
- **api_key 明文**：config.json 本身明文存储（现状如此），本次仅做 UI 掩码，不引入加密（如需加密另行立项）。
- **布局破坏**：合并视图的迁移为一次性，用户自定义布局中的 providers/models 面板会被合并去重，属于预期行为，迁移逻辑需覆盖 `migrateLegacyKey` 各入口。
- **并发写**：`save_provider_config` 与模型调用并发时，reload 采用不可变快照 + 原子替换，避免读写竞态。

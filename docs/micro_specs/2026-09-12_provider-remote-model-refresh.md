# 服务商远端模型列表拉取 + 三种合并策略

- 日期：2026-09-12
- 状态：已实施，待人工走查（见 §9）
- 影响包：`packages/pulsar-app`（Rust 后端 + Svelte 前端）
- 关联：[2026-08-14_provider-model-unified-management.md](2026-08-14_provider-model-unified-management.md)

## 1. 目标

在 `ProviderManager` 编辑器内提供「刷新模型列表」：按 OpenAI 规范请求 `{api_base}/models` 拉取该服务商真实可用模型，弹窗展示，并提供三种合并策略一键写回编辑器草稿。

## 2. 事实与约束

- OpenAI 规范只有 `GET /v1/models`（List models），**没有**「服务商列表」接口；远端仅返回 `id / object / created / owned_by`，**不含** `capabilities / context_window / thinking / pricing`。
- 因此三种策略的差异本质是「同名模型（key = 模型 id）保留谁的元数据」+「本地独有模型是否保留」。
- `openai_compat::Client` 当前 `endpoint()` 写死 `/chat/completions`，无 `/models` 能力。
- api_base / api_key 解析复用 `ProviderRegistry::resolve_provider_config`（env 优先，回落 config，再回落内置默认）。
- 编辑器为「改草稿 → 保存才写盘热重载」模式；本次拉取与合并**只改草稿**，不触发落盘。
- 命令需三处同步：`lib.rs`(Tauri) / `net/rpc.rs`(WebRPC) / `contracts.ts`(前端契约)。

## 3. 已确认决策

| 决策点 | 结论（用户确认） |
| --- | --- |
| 生效时机 | **只改编辑器草稿**，仍需点底部「保存」才写盘 |
| 云端覆盖本地 | **追加语义**：本地独有模型保留；同名模型 key 以云端为准（条目按云端重建，元数据回落默认） |
| 本地优先 | 并集；同名保留**本地**全部配置；云端独有追加为默认条目 |
| 重置为云端 | 完全按云端重建：本地独有模型全部移除，同名亦重置为默认条目 |

## 4. 方案

### 4.1 后端

1. `core/models.rs`：新增 `RemoteModelInfo { id, owned_by?, created? }`（`GET /models` 单条，跨层传输类型）。
2. `providers/openai_compat.rs`：`Client` 新增 `list_models()`，请求 `{base}/models`，Bearer 鉴权；响应 `{ data: [...] }` 解包为 `Vec<RemoteModelInfo>`；复用 `read_body` / `map_error` 做错误归一。
3. `providers/providers.rs`：`ProviderRegistry` 新增 `fetch_remote_models(provider_id) -> AppResult<Vec<RemoteModelInfo>>`；provider 不存在 → `provider_not_found`；api_base 回落规则与 `call_model` 一致。
4. `lib.rs`：新增 Tauri 命令 `list_remote_models(provider_id)` 并注册。
5. `net/rpc.rs`：新增 `"list_remote_models"` 分支。

只读命令，不广播 `StateChange`。

### 4.2 前端

6. `types.ts`：新增 `RemoteModelInfo` 类型。
7. `contracts.ts`：新增 `listRemoteModels: def<{ providerId: string }, RemoteModelInfo[]>("list_remote_models")`。
8. `i18n/translations.ts`：补 `providerManager.refreshModels` / `remoteModelsTitle` / `remoteModelsEmpty` / `remoteModelsCount` / `mergeCloudOverwrite(+Hint)` / `mergeLocalFirst(+Hint)` / `mergeResetCloud(+Hint)` / `close`（类型 + en + zh 三处）。
9. `ProviderManager.svelte`：
   - 模型区标题栏新增「刷新模型列表」按钮（草稿 provider `id === ""` 时禁用）。
   - 点击 → 调 `listRemoteModels`；成功后打开弹窗（复用 `ConfirmDialog` 的 overlay/surface 词汇），展示 id 列表（可滚动、显示条数、`owned_by` 次要信息）。
   - 弹窗三按钮执行合并（见 §3），完成后关闭弹窗并保持草稿脏状态。
   - 拉取失败复用现有 `error` banner 展示。

## 5. Done Contract

- 完成即：编辑器内可对任一已存服务商拉取远端模型并弹窗展示；三个按钮各自改写 `selected.models` 草稿；`cargo test -p pulsar-app` 与前端 `svelte-check` 通过。
- 由什么证明：新增后端单测（`/models` 响应解析、provider 不存在报错）+ 手动验证弹窗与三种合并结果。
- 什么算未完成：三按钮行为无法区分；拉取直接落盘；仅 Tauri 通道可用而 WebRPC 通道缺失。

## 6. 验证方式

- `cargo test`（后端单测）
- `pnpm --filter pulsar-app check`（前端类型检查）
- 手动：对 `deepseek` / 自建 `api_base` 分别拉取；对比三按钮结果与草稿；确认不点保存不落盘。

## 7. Change Log

- 2026-09-12：初版，用户确认三按钮语义与「只改草稿」生效时机。
- 2026-09-12：实现完成。后端 `openai_compat::Client::list_models` + `parse_models_response`（可测）；
  `ProviderRegistry::fetch_remote_models`；Tauri 命令 `list_remote_models` 与 WebRPC 分支注册；
  `RemoteModelInfo` 落 `core/models.rs` 并由 `core/mod.rs` 再导出。
  前端 `types.ts` / `contracts.ts` / i18n（新增 `common.close`）与 `ProviderManager.svelte`
  （模型区标题栏刷新按钮 + 浮层弹窗 + 三策略合并）。策略实现：
  「云端覆盖本地」保留本地独有条目、同名条目按云端重建；「本地优先」同名保留本地配置；
   「重置为云端」完全按云端重建。
- 2026-09-12：调整——保存服务商配置后**不再关闭面板**；因面板常驻，补充「保存后按 id 复位选中」
  （自定义服务商经 config `HashMap` 往返后顺序不保证，按索引会串选）。
- 2026-09-12：按实测响应补 `display_name`。实测网关 `/models` 返回
  `{id, object, created, owned_by, type, display_name}`：官方 4 字段 + 两个非标准扩展。
  `type` 恒为 `"model"`（与 `object` 重复）故忽略；`display_name` 有价值
  （`gpt-6-astra` → `GPT-6 Astra`）故接入：`RemoteModelInfo` 加可选 `display_name`，
  前端 `remoteToModel` 以其作显示名、缺失回落 `id`，弹窗中 id 旁附显示名。
  未接入 `context_length` / `pricing` / `supported_parameters`——实测该网关不下发，
  且属 OpenRouter 系非标准字段。另注意：实测 `created` 大量为 `1704067200`（2024-01-01）占位值，
  **不可用于排序或判断新旧**。

## 8. Validation

- `cargo check`：通过。
- `cargo test --lib providers::`：19 passed / 0 failed（新增 3 条：models 响应解析、畸形响应拒绝、未知服务商拉取报错）。
- `pnpm --filter pulsar-app check`：0 errors（ProviderManager 无新增告警）。
- 未做：真机 UI 走查（需 `pnpm tauri:dev` 并配置可用服务商），留待人工验证。

## 9. 人工验证清单（未完成项）

1. 打开 main 区编辑器，选中任一已存服务商 → 模型区标题栏出现刷新按钮；新建草稿（id 为空）时按钮禁用。
2. 配置指向可用的 `api_base` + key，点刷新 → 弹窗列出云端模型与条数；不可达时错误进顶部 banner。
3. 依次验证三策略对草稿 `models` 的改写符合 §3 表格；关闭弹窗后草稿仍为脏状态，未点「保存」不写盘。

# Spec: 统一 config.json 读写入口 + 结构化输出探测热更新 + 400 错误驱动降级

- 日期：2026-08-27
- 状态：已执行完成（2026-08-27 23:xx）
- 深度：standard（架构收敛 + 跨模块）

## 1. 背景与问题（Reverse Sync：实现偏差已确认）

实测会话 `conv_1787843280657116022` / `conv_1787844605685241445` 暴露三个问题：

1. **双套 config.json 读写**：`ConfigStore`（config.rs）注释自称「统一读写入口」，但
   `ProviderManager`（providers.rs）自建 `read_config`/`write_config`（L715/L726）并行
   实现另一套 fs 读写，两者操作同一文件。架构承诺与现实不符，存在解析/序列化逻辑漂移
   与并发写竞态风险。
2. **探测缓存永不失效**：`assistant_session.rs::structured_output_support` 用
   `static OnceLock<Mutex<HashMap>>` 缓存 `(provider_id, model_id)` → 支持级别，命中
   即返回、不重读 config。用户把模型 JSON 开关改为 false（config.json 已落盘），不重启
   仍持续下发 `json_schema` → 400 → 降级。
3. **400 重试必败**：`call_judgement` 的 B 重试把错误回灌 payload 但 `response_format`
   原样重发，请求层被拒（模型未开口）时两次尝试全浪费，随后 neutral_fallback 降级为
   `none`，课题管线整轮缺席（match_topic 不建课题，revise/complete_scope 级联 skip）。

## 2. 目标

1. **统一配置 IO**：`ConfigStore` 成为 config.json 唯一读写入口；`ProviderManager`
   收敛为委托 ConfigStore，删除自己的 fs 读写。
2. **配置热更新（硬需求）**：修改 provider/model 配置（含能力开关）后，结构化输出
   探测结果立即按新配置生效，无需重启。
3. **错误驱动降级（治本）**：判定调用遇 response_format 类 400 时，自动降一级
   （json_schema → json_object → 无约束）、写回探测缓存，并以降级后格式重试。

## 3. 设计决策

### 3.1 IO 收敛（无循环依赖）

- `providers` / `defaults` 顶层键在 `ConfigStore` 侧保持 **Value 级承载**（已由
  `extra` flatten 天然保留，无需为 provider 类型建模）。
- `ProviderManager` 从 `ConfigStore::read()` 的 `extra` 中提取 `defaults`/`providers`
  原始 Value，再 serde 反序列化为内部 `AppConfig` 类型。
- 依赖方向：providers.rs → config.rs（单向，已存在）；config.rs 不引入任何
  providers.rs 类型。**无循环依赖**。

### 3.2 原子写统一

- `ConfigStore::update` 升级为 **tmp + rename 原子写**（对齐 ProviderManager 现有
  write_config 语义，L748-749），统一后不损失写可靠性。

### 3.3 配置代数计数器（热失效机制）

- `ConfigStore` 增加进程级计数器：`static ONCE: OnceLock<AtomicU64>`。
- `update()` 写盘成功后该代数 +1；提供 `pub fn generation() -> u64` 只读方法。
- 探测缓存键由 `(provider_id, model_id)` 扩为 `(provider_id, model_id, generation)`：
  任何程序侧配置写入 → 代数变化 → 缓存自然 miss → 重读 config 探测。无需事件广播。
- 语义：**config.json 被程序成功写盘过 N 次**。所有 ConfigStore 实例（临时 `new`
  创建）共享同一进程级计数器。

### 3.4 错误驱动降级

- `call_judgement` 首轮 `Err` 且错误文本含 `response_format` 时：当前支持级别降一级
  （`JsonSchema→JsonObject→None`）→ 覆盖写回探测缓存（仍带当前代数）→ attempt 2
  改用降级后的 `response_format`（复用现有 B 重试位，仅替换格式）。
- 已到 `None` 级别时无可降，维持原重试行为；attempt 2 仍失败则维持 neutral_fallback。

### 3.5 明确不做

- **文件 watch**：手工直接改 config.json 仍需重启（现状如此，本次不引入 watcher）。
- **并发写 Mutex 串行化**：当前无并发写场景；统一到单入口后风险已实质降低，留待有
  真实并发用例时再加。
- `assistant_select_neuron` builtin seed 缺口（weight fallback 常态化）另立任务。
- json_object 翻译层（spec 2026-08-23 决策 2 预留点）本次不动。

## 4. 涉及文件

| 文件 | 改动 |
| --- | --- |
| `src/core/config.rs` | `update` 原子写；进程级代数计数器 + `generation()` |
| `src/core/providers.rs` | 删 `read_config`/`write_config` fs 实现；改为经 ConfigStore 读写（extra Value 级）；`save_config` 委托 `ConfigStore::update` |
| `src/core/assistant_session.rs` | 探测缓存键加代数；`call_judgement` 400 错误驱动降级 |

## 5. 改动要点

### 5.1 config.rs

- 进程级代数：`static GENERATION: OnceLock<AtomicU64>`，`generation()` 返回当前值。
- `update()`：闭包改内存 `AppConfigFile` → 序列化 → 写 `config.json.tmp` → `rename`
  → 成功后 `GENERATION +1`。

### 5.2 providers.rs

- 新增 `fn read_app_config(&self) -> AppResult<AppConfig>`：`ConfigStore::new(&root)
  .read()` → 从 `file.extra` 取 `defaults` / `providers` Value → serde 成 `AppConfig`
  （缺失键回落默认），替代原 `read_config()` 全部调用点（L192/L240/L267/L286/L298/
  L310/L325/L535/L684）。
- `save_config`：删除 `write_config`，改为
  `ConfigStore::new(&root).update(|c| { 写 defaults/providers 到 c.extra })`，随后
  保留 `reload()` 与 `get_config_view()`。`build_providers_json` 的旧值 root 参数改从
  闭包内 `c.extra` 获取（保持 api_key 掩码保留语义）。
- `defaults` 删除语义（view.defaults 为 None → 移除 `defaults` 键）同步保留。
- 现有单测（save_config / api_key 掩码 / 校验）语义不变，须全量通过。

### 5.3 assistant_session.rs

- `structured_output_support` 缓存键扩为 `(String, String, u64)`（第三位
  `ConfigStore::generation()`）；探测逻辑不变。
- `call_judgement`：首轮失败分支中识别「response_format 类 400」（错误文本含
  `response_format`），命中则降级并缓存，attempt 2 用降级格式。

## 6. 验证方式

- `cargo test --lib`（pulsar-app src-tauri）全量通过，含新增单测：
  1. `ConfigStore::update` 成功后 `generation()` 递增；
  2. `save_config`（改 `structured_output` true→false）后，以新代数重探得到
     `JsonObject`（模拟热失效）；
  3. 400 降级：首轮错误含 `response_format` → attempt 2 请求不再携带 `json_schema`。
- grep 验证 config.json 仅剩 ConfigStore 一处写入口。
- 运行时（用户侧）：不重启改 JSON 开关 → 日志 `llm_request_out` 即时反映；
  DeepSeek（`structured_output=false`）判定调用不再出现
  `response_format type is unavailable` 400。

## 7. 风险

- `build_providers_json` 旧值来源由 fs root 改为 extra，需单测覆盖 api_key 掩码保留。
- 默认 providers：config 缺失回落内置全集，委托 ConfigStore 后行为等价（read 失败
  语义保持）。
- 错误文本启发式匹配 `response_format` 可能误判：后果仅是「下次调用少一层结构化
  约束」，可由用户改回开关恢复（代数机制保证配置可重新生效），风险可接受。

## 8. Done Contract

- [x] `cargo test --lib` 通过（414 passed，含 3 个新增单测，现有单测不破坏）
- [x] grep 确认 config.json 仅剩 `ConfigStore` 一处写入口
- [x] 保存 provider 配置后探测结果无需重启即生效（单测 `structured_output_probe_invalidated_by_config_update` 证明）
- [x] DeepSeek 关闭 JSON 开关后判定调用不再 400（单测 `response_format_400_downgrades_retry_with_json_object` 证明；真实端点待用户侧运行时确认）
- [x] 执行后回写本 spec（Change Log / Validation）

## 10. Change Log（执行记录）

- 2026-08-27 批准执行。
- config.rs：新增进程级代数计数器（`config_generation` + `pub config_generation_value()`）；
  `update()` 升级为 tmp+rename 原子写，写盘成功后代数 +1。
- providers.rs：删除自建 `read_config`/`write_config` fs 实现，改为委托 `ConfigStore`
  （`defaults`/`providers` 段 Value 级承载于 `extra`，`app_config_from_file` 反序列化）；
  `build_providers_json` 旧值参数由整文件 root 收窄为旧 providers 段。删除主代码 `use std::fs`。
- assistant_session.rs：`structured_output_support` 缓存键扩为
  `(provider_id, model_id, config_generation)`（`capability_cache` 进程级 static）；
  新增 `degrade_structured_output_support`（400 响应后降级写回缓存）与
  `is_response_format_error`（错误文本启发式匹配）；`call_judgement` 重试分支错误驱动
  降级（attempt 2 改用降级后格式）。
- 新增单测：config 代数递增 + 原子写；save_config 热失效（true→false 立即降级）；
  400 → attempt 2 改发 json_object。全量 414 通过。
- 删除被取代的 micro-spec（23-38）。

## 11. Validation

- `cargo test --lib`：414 passed；0 failed。
- `cargo check --lib`：仅 2 个 pre-existing unused-import warning
  （`MessageContent`/`Cow` 仅测试代码使用，非本次引入）。
- 运行时验证（未执行，需用户侧确认）：
  - 不重启改模型 JSON 开关 → 下一轮判定调用即按新级别下发（`llm_request_out` 日志）；
  - DeepSeek 端点 `structured_output=false` 时判定调用不再出现 400
    `response_format type is unavailable`。

## 9. 相关文档

- 原 micro-spec `docs/micro_specs/2026-08-27_23-38_structured-output-probe-hot-reload.md`
  已被本 spec 取代并删除，避免双真相源。
- 关联：`docs/micro_specs/2026-08-23_11-00_response-format-json-schema-wire.md`
  （wire 形态与降级链背景）。

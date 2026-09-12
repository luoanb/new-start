# 阻塞项原因可见性 + 简报跳过/终止规则

- 日期：2026-09-12
- 范围：`TopicPanel.svelte` / `types.ts` / `i18n/translations.ts`；`build_topic_brief`（`assistant_session.rs`）；`docs/pulsar/assistant/index.md` §3
- 状态：已落地

## 背景 / 问题

`ScopeInItem.blocked_reason`（后端 `models.rs`，标记 blocked 时记录「用户需要做什么」，解除阻塞时清空）**后端已在 wire 里下发**（`skip_serializing_if = "Option::is_none"`），但：

1. **用户看不到**：前端 `ScopeInItem` 类型根本没有该字段，`TopicPanel` 只渲染一个 `blocked` 状态徽标——用户看不到"为什么在等自己"，也就无从介入解除。
2. **模型未被明确告知要跳过**：`build_topic_brief` 已把 blocked 项渲染为 `[⏳] 等待用户` + `需要：<原因>`，但「本轮任务」指令只说"选择一件尚未完成的事项推进"，**没有说 [⏳] 项本轮不要选**——模型可能去啃等待用户的事项，或把它直接标完成。
3. **终止规则未告知模型**：终止条件是"没有 pending 项、但有 blocked 项"（推导态 `waiting_user` → Poller 跳过该课题）。模型不知道这条规则，就可能①在仍有 pending 项时提前停工，或②把还在推进的项随手标成 blocked 而让整个流程停摆。

## 修复

### 1. 课题面板展示阻塞原因（前端）

- `types.ts`：`ScopeInItem` 补 `blocked_reason?: string | null`。
- `translations.ts`：新增 `scopeBlockedReason`（类型 + en `Needs: ` / zh `需要：`）。
- `TopicPanel.svelte`：blocked 项在验收标准下方追加一行原因（`warning` 色、单行省略、`title` 兜全文），与 `.scope-contract` 同款排版。

### 2. 简报补两条规则（`build_topic_brief` 常规推进分支）

在原有「本轮任务」后追加：

- `[⏳]` 项正在等待用户介入 → 本轮**不要选择、不要标记完成**，直接跳过（理由已附在项后）。
- 只要还有 `[ ]` 待办项就继续推进；**仅当所有未完成项都是 `[⏳]`** 时本轮流程才终止（等待用户介入后自动恢复）。

`WrappingUp` 分支（收尾总结、无需调工具）不变。

## 验证

- `cargo test --lib`：`topic_brief_marks_blocked_items_and_skips_normal_instruction` 扩展断言（跳过规则 + 终止规则文案），并新增 `topic_brief_tells_model_to_keep_going_when_pending_remains`。
- `pnpm check`（svelte-check）：前端类型与模板无错。
- 人工：展开课题 → blocked 项下方可见"需要：…"；解除阻塞（用户接入）后该行消失。

## 追加（同一轮对话）：原因是否保留 + 数据源头

### 排查结论

`blocked` 项的四种出路，原因去留不同：

| 出路 | 入口 | `blocked_reason` |
|---|---|---|
| 用户介入解除阻塞 | `unblock_scope_items` | 清空（有意设计：全回 pending、原因抹掉） |
| 被验收（blocked → completed） | `complete_scope_item`（`round_review` / GUI / RPC 均可达） | **残留**（只改 status） |
| 再次标阻塞 | `mark_scope_item_blocked` | 覆盖：给新原因换新，传 None 则清空 |
| 编辑 / 删除条目 | `update_scope_item` / remove | 编辑保留；删除随项消失 |

且**原因大概率一开始就没写进去**：`blocked_reasons` 虽在裁决 JSON schema 的 required 里，但 `config.rs` 的系统提示词与 `inserts/assistant.round_review.md` **都没把它列入必须字段、也没说明用途** → 模型给空对象 → `blocked_reason = None`。

### 追加修复

1. **补提示词**（用户决策：不做后端兜底校验）——两处都要改，因为模型读到的是「DB 神经元 content（源自 `config.rs`）+ insert 契约」：
   - `policies/neuron/config.rs` 的 `assistant_round_review` 种子：职责二标题补 `blocked_reasons`、JSON 示例补该字段、新增「标记 blocked 必须同时给出原因（写明用户需要做什么）」与「必须覆盖 blocked_item_ids 全部 id」两条；
   - `inserts/assistant.round_review.md`：必须字段清单、JSON 示例、规则同步。
2. **保留历史，仅修简报**（用户决策）——`complete_scope_item` 不动（残留原因作审计痕迹），改 `build_topic_brief`：**原因只对 `status == "blocked"` 的项投影**，避免已完成项在简报里带 `需要：…` 误导模型。

### 追加验证

- `cargo test --lib` → **469 passed / 0 failed**（新增 `topic_brief_hides_stale_reason_on_non_blocked_items`）。
- 生效前提：`config.rs` 改的是**种子**，运行时 `ensure_system_neuron(reset: false)` 不会覆盖已有 DB 神经元——需触发一次重置（GUI「重置系统提示词」/ TUI `/neuron rebootstrap` 或 `/neuron reset-system assistant_round_review`）后新提示词才进库。

# Spec: ProviderManager 选中身份与可编辑 id 解耦

## Goal

- 要解决什么问题：`ProviderManager.svelte` 用 `p.id === selectedId` 匹配选中项，而 id 输入框直接 `bind:value={selected.id}`。每敲一个字符都在改写匹配键 → `selected` 变 `null` → 表单面板整体卸载。新建流程因保存按钮 `disabled={saving || creating}`（草稿必须先填 id）被完全卡死；表单消失后再点「+」还会插入重复草稿。
- 本次目标：选中身份改为索引制（`selectedIndex`），id 恢复为纯数据字段；新建/编辑 id 全程表单不丢。
- 验收结果：`pnpm check` 无新增错误；新建服务商：输入 id 全程表单保持、保存可用；编辑既有服务商 id 同样不丢；侧栏「编辑/新增」请求跳转正常。

## Done Contract

- 完成：单文件改造 `ProviderManager.svelte`（选中态 7 处消费点）。
- 由什么证明：`pnpm check` 通过 + 代码走查（无 `selectedId` 残留）。
- 哪些情况仍算未完成：仍有按 id 匹配选中的残留；`creating` 语义被改变（保存按钮门槛失效）。

## Scope

- In：`ProviderManager.svelte` 前端单文件。
- Out：后端 `save_provider_config` / 校验逻辑、`ProvidersModelsPanel`、布局持久化、i18n。

## Facts / Constraints

- `creating` 语义 =「选中的是未保存草稿（id === \"\")」，用于禁用保存按钮（L518），必须保留。
- 侧栏「编辑」请求（`providerEditRequestId`）按 id 传入 → 打开时一次性换算为索引即可。
- 保存成功后编辑器立即关闭（`closeProviderManager`），保存后选中漂移无影响。

## Restated Understanding

- 我理解当前任务是：修复服务商 id 输入导致编辑面板丢失的 UI bug。
- 当前核心目标是：选中身份与 id 解耦（索引制），新建/编辑全程表单稳定。
- 当前边界是：仅前端单文件，不改后端与其它面板。
- 暂不处理：保存后列表排序漂移（编辑器随即关闭，无实际影响）。

## 接口契约设计

```ts
// 选中态：索引制
let selectedIndex = $state<number | null>(null);
const selected = $derived(selectedIndex !== null ? (view.providers[selectedIndex] ?? null) : null);
const creating = $derived(selected?.id === "");
```

## Checkpoint Summary

- 当前进度：根因定位完成，方案获用户批准；待实现。
- 下一步 1: 改造 7 处消费点（声明/派生、load、两个 $effect、removeProvider、加号按钮、列表项高亮与点击）。
- 下一步 2: `pnpm check` 验证。
- 风险：无后端改动；`creating` 语义保持不变。
- 验证方式：`pnpm check` + 走查无 `selectedId` 残留。
- Execution Approval: `Approved`（用户 2026-08-27 选择"按方案修复"）

## Change Log

- 2026-08-27: 初始 spec。根因：选中按可编辑 id 匹配，输入即断链。

## Validation

- Self-check: 7 处消费点全部改造；`selectedId` 零残留（grep 确认）；`creating` 语义保持（`selected?.id === ""`，保存按钮门槛不变）。
- Static checks: `pnpm check` 0 errors（20 warnings 为既有历史警告，与本次无关）。
- Runtime / Test: 无自动化 UI 测试覆盖该组件；建议人工验证：新建服务商输入 id 全程表单不丢、保存可用；编辑既有 id 不丢；侧栏编辑/新增跳转正常。
- Human confirmation: 方案已批准并实现。
- 结果汇总：实现与静态验证完成。
- 核心目标是否已由证据证明完成：是——选中身份改为索引制，id 输入不再断链。
- 若未完成，当前剩余差距：无。
- 剩余风险：保存后编辑器立即关闭，选中漂移无实际影响（已评估）。

## Resume / Handoff

- 当前状态：修复完成，静态验证通过。
- 当前卡点：无。
- 下一步唯一动作：人工走查新建/编辑 id 场景（可选）。

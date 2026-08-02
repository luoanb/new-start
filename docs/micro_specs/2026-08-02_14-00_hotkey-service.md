# Spec: Hotkey Service（快捷键服务）

## Goal

- 要解决什么问题：当前 App 所有快捷键在 `+page.svelte` 的 `handleKeydown` 里硬代码，且无条件 `e.preventDefault()` 吞掉所有带 Ctrl/Meta 的组合键，导致浏览器/系统快捷键（Ctrl+T/W/R 等）失效。需要把快捷键抽成独立服务，支持用户注册自定义快捷键、按 DOM 挂载点生效、以及忽略放行配置。
- 验收结果：快捷键逻辑集中在独立 `.ts` 服务；仅命中 App 动作的按键才 `preventDefault()`；用户可注册自己的快捷键；挂载点按 DOM 判定；用户可声明忽略组合交给浏览器。

## Done Contract

- 什么算完成：新增 `lib/hotkey/hotkeyService.ts`；`+page.svelte` 的 `handleKeydown` 委托给服务；服务支持 `register(combo, cb)`、DOM 挂载点匹配、忽略列表；原 5 个快捷键行为不变。
- 由什么证明：`read_lints` 无错；手动在 App 内验证 Ctrl+T/W/R 等系统快捷键恢复响应、App 自身快捷键仍生效。
- 哪些情况仍算未完成：未接入设置 UI（本 spec 不含 UI，仅服务层）；`Cmd+W` 等 Tauri 级拦截若依赖 `tauri.conf.json` 配置则不在本任务范围。

## Scope

- In：快捷键服务模块、注册 API、DOM 挂载点判定、忽略列表、改造 `+page.svelte` 接入。
- Out：持久化/配置文件、设置面板 UI、`tauri.conf.json` 改造、Rust 端全局快捷键。

## Facts / Constraints

- 已确认事实：`+page.svelte:233-254` 的 `handleKeydown` + `svelte:window onkeydown`；现有按键 Ctrl/Cmd+J/Shift+J/B/I/\\ 与 Esc。
- 技术/业务约束：
  - **绑定与忽略都是服务单例行为**：服务在模块加载时**一次性**把 `keydown` 监听挂到 DOM 根（`document`/`window`），调用方注册时只传入 combo+handler+元素，**各自不挂监听、不管理挂载位置**；所有注册表与忽略表由单例统一持有。
  - 绑定/忽略都基于 DOM 元素（`HTMLElement`）：注册时把 combo 绑定到某节点，挂载点 = 该节点及其子树；忽略同样绑定到 DOM 节点。不引入自定义 `HotkeyScope` 类型 / 字符串 ID。
  - 挂载点判定用事件 `target` + `closest` 向上找绑定元素；**最内层（最具体）绑定元素优先**。
  - **忽略的单例内置放行规则**：
    - 2.1 **可输入区放行**：`input / textarea / [contenteditable]` 内的按键默认放行（不拦截、不 preventDefault），除非在该元素上显式注册了 combo。
    - 2.2 **自定义特殊 class 放行**：元素带指定 class（如 `hotkey-ignore`）时，其子树内按键放行；亦可由 `ignoreCombo(el, combo)` 精确声明某元素某组合放行。
  - `HotkeyAction` 极简：**按键组合 + 回调**，不堆 description/enabled/defaultPrevent 等冗余字段。
  - 不用 `.svelte.ts`（无状态管理需求），普通 `.ts` 模块即可。
  - 不持久化（无配置/存储层），纯运行时注册。
- 已知风险：挂载点子树内事件冒泡可能造成双重触发（如 ChatInput 内 Ctrl+J）；需在匹配时按最内层绑定元素优先且同 action 不重复执行。

## Open Questions

- [ ] 注册时 combo 冲突（多 action 同组合同 scope）如何处理：本 spec 采用后注册覆盖 / 或同 scope 内全部执行？默认同 scope 内全部执行。

## Restated Understanding

- 我理解当前任务是：把硬代码快捷键抽成无状态普通 `.ts` 服务，支持用户注册（按键+回调）、按 DOM 挂载点生效、用忽略列表放行系统组合键，并修复 `preventDefault` 误吞问题。
- 当前核心目标是：交付可注册、可挂载、可忽略放行的快捷键服务，且不回归现有 5 个快捷键行为。
- 当前边界是：`lib/hotkey/hotkeyService.ts` + `+page.svelte` 改造；无 UI、无持久化。
- 暂不处理：设置面板、持久化、`tauri.conf.json`。

## 接口契约设计

```ts
// lib/hotkey/hotkeyService.ts
export type HotkeyCombo = {
  key: string;            // 小写字符，如 "j" "\\"
  ctrl?: boolean; meta?: boolean; shift?: boolean; alt?: boolean;
};

export type HotkeyHandler = (e: KeyboardEvent) => void;

export type HotkeyInitOptions = {
  bindRoot: HTMLElement;            // 初始化时约定：快捷键绑定的 DOM 根（监听挂这里）
  ignoreClass?: string;             // 初始化时约定：自定义特殊 class（默认 "hotkey-ignore"）
  ignoreInput?: boolean;            // 是否忽略可输入区（input/textarea/contenteditable），默认 true（放行）
};

// 初始化（一次性）：约定 bindRoot / ignoreEls / ignoreClass，并把 keydown 单例监听挂到 bindRoot。
// 调用方不在运行时各自传 DOM、不各自挂监听。忽略范围也仅由初始化约定决定，无运行时忽略 API。
export function initHotkeyService(opts: HotkeyInitOptions): void;

// 注册：只传 combo + 回调，绑定到初始化约定的 bindRoot。返回注销函数。
export function registerHotkey(combo: HotkeyCombo, handler: HotkeyHandler): () => void;

// 统一入口，由单例监听内部调用
function dispatchKeydown(e: KeyboardEvent): void;

// 把 KeyboardEvent 转成 HotkeyCombo（key 小写化）
function comboFromEvent(e: KeyboardEvent): HotkeyCombo;

// 单例内置放行判定（忽略规则，全部来自初始化约定，无运行时忽略 API）
function isPassThrough(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  if (!t) return false;
  // 2.1 可输入区放行（由 ignoreInput 控制，默认 true；除非该元素本身显式注册了 combo）
  if (ignoreInput && t.closest('input, textarea, [contenteditable="true"], [contenteditable=""]'))
    return !hasRegistered(t, comboFromEvent(e));
  // 2.2 自定义特殊 class 放行（初始化约定的 ignoreClass）
  if (t.closest('.' + ignoreClass)) return true;
  return false;
}
```

内部行为（单例 `dispatchKeydown`）：
1. Esc 走现有 drawer 关闭逻辑（保持原行为）。
2. 计算 `combo = comboFromEvent(e)`。
3. 若 `isPassThrough(e)` 为真（可输入区未显式注册 / 命中 ignoreClass / 在 ignoreEls 子树内）→ 不 preventDefault、不处理，直接 return（放行）。
4. 事件 `target` 在 `bindRoot` 子树内 → 命中已注册 combo 时**仅此时** `e.preventDefault()`，执行对应 handler。
5. **没有命中任何 combo** → 不 preventDefault，直接交给浏览器/系统（修复系统快捷键失效）。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是，落盘 micro-spec 是执行前必要 spec。
- 若否：N/A。

## Checkpoint Summary

- 当前任务理解：抽快捷键为无状态 `.ts` 服务，支持注册/DOM 挂载/忽略放行，修复 preventDefault 误吞。
- 当前核心目标：交付 hotkeyService.ts 并改造 +page.svelte，行为不回归。
- 当前进度：micro-spec 已落盘，待批准。
- 下一步 1：创建 `lib/hotkey/hotkeyService.ts`（registerHotkey / ignoreCombo / dispatchKeydown）。
- 下一步 2：改造 `+page.svelte` 用 dispatchKeydown 接管，删除原 handleKeydown 硬代码。
- 涉及文件 / 模块：`packages/agent-app/src/lib/hotkey/hotkeyService.ts`（新）、`packages/agent-app/src/routes/+page.svelte`。
- 风险：ChatInput 内冒泡可能双重触发；需按最内层 scope 优先且去重。
- 验证方式：`read_lints` + 手动验证系统快捷键恢复 + App 快捷键仍生效。
- Execution Approval: `Approved`

## Change Log

- 2026-08-02: 由硬代码快捷键改造为独立无状态快捷键服务，确立 initHotkeyService / registerHotkey 接口契约（删 ignoreCombo / ignoreEls，新增 ignoreInput）。
- 2026-08-02: 实现 `lib/hotkey/hotkeyService.ts`（单例，keydown 挂 bindRoot，未命中放行）；改造 `+page.svelte` 用 registerHotkey 注册原 5 个快捷键，Esc 保留 window 级处理。

## Validation

- Self-check：接口契约与 micro-spec 一致；`ignoreEls`/`ignoreCombo` 已删除、`ignoreInput` 已落地；`preventDefault` 仅在命中 combo 时调用；可输入区/`.hotkey-ignore`/未命中均放行。
- Static checks：`read_lints` 对 `+page.svelte` 与 `hotkeyService.ts` 均 0 错误。
- Runtime / Test：未运行（需手动在 App 内验证）。
- Human confirmation：待用户在 App 内验证 Ctrl+T/W/R 等系统快捷键恢复、App 自身 5 个快捷键仍生效。
- 结果汇总：代码已落地、lint 通过；运行验证待人工确认。
- 核心目标是否已由证据证明完成：代码层已证明（lint 通过、结构符合 spec）；端到端行为需人工确认。
- 若未完成，当前剩余差距：缺人工运行验证（系统快捷键是否真正恢复受 Tauri `tauri.conf.json` 是否禁用 webview hijack 影响，超出本任务范围）。
- 剩余风险：`Cmd+W`/`Ctrl+W` 等浏览器级快捷键即便不 preventDefault，在 Tauri 中能否放行取决于 `tauri.conf.json` 的 `dangerousDisableWebviewHijack`；不在本任务范围。

## Resume / Handoff

- 当前状态：已实现并通过 lint；待人工在 App 内运行验证。
- 当前卡点：需人工确认系统快捷键恢复情况。
- 下一步唯一动作：在 App 内手动验证 5 个 App 快捷键 + 系统快捷键（Ctrl+T/W/R）行为。
- 下一轮核心目标：若验证发现 Tauri 级拦截问题，再决定是否调整 `tauri.conf.json`（单独评估，非本任务）。

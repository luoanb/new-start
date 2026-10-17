// 快捷键服务（单例）。初始化时约定绑定的 DOM 根与忽略规则，运行时只注册 combo + 回调。
// 设计要点（来自 spec）：
// - 绑定/忽略都基于 DOM，由 initHotkeyService 一次性约定，调用方不各自挂监听。
// - 未命中任何 combo：不 preventDefault，直接交给浏览器/系统（修复系统快捷键被吞）。
// - 命中 App 动作：仅此时 preventDefault 并执行回调。

export type HotkeyCombo = {
  key: string; // 小写字符，如 "j" "\\"
  ctrl?: boolean;
  meta?: boolean;
  shift?: boolean;
  alt?: boolean;
};

export type HotkeyHandler = (e: KeyboardEvent) => void;

export type HotkeyInitOptions = {
  bindRoot: HTMLElement; // 快捷键绑定的 DOM 根，keydown 单例监听挂这里
  ignoreClass?: string; // 自定义特殊 class（默认 "hotkey-ignore"），命中即放行
  ignoreInput?: boolean; // 是否忽略可输入区（input/textarea/contenteditable），默认 true（放行）
};

type ComboKey = string;

function comboKey(c: HotkeyCombo): ComboKey {
  return [
    c.key.toLowerCase(),
    c.ctrl ? 1 : 0,
    c.meta ? 1 : 0,
    c.shift ? 1 : 0,
    c.alt ? 1 : 0,
  ].join("|");
}

function comboFromEvent(e: KeyboardEvent): HotkeyCombo {
  return {
    key: e.key.toLowerCase(),
    ctrl: e.ctrlKey,
    meta: e.metaKey,
    shift: e.shiftKey,
    alt: e.altKey,
  };
}

// ── 单例状态 ──
let initialized = false;
let bindRoot: HTMLElement | null = null;
let ignoreClass = "hotkey-ignore";
let ignoreInput = true;
const registry = new Map<ComboKey, HotkeyHandler[]>();
// 记录每个 combo 是否有显式注册在「可输入区元素」上，用于 isPassThrough 的例外判定
const inputRegisteredKeys = new Set<ComboKey>();

function initHotkeyService(opts: HotkeyInitOptions): void {
  bindRoot = opts.bindRoot;
  ignoreClass = opts.ignoreClass ?? "hotkey-ignore";
  ignoreInput = opts.ignoreInput ?? true;

  if (initialized) return;
  initialized = true;
  bindRoot.addEventListener("keydown", dispatchKeydown);
}

/** 注册快捷键：只传 combo + 回调，绑定到初始化约定的 bindRoot。返回注销函数。 */
function registerHotkey(combo: HotkeyCombo, handler: HotkeyHandler): () => void {
  const key = comboKey(combo);
  const list = registry.get(key) ?? [];
  list.push(handler);
  registry.set(key, list);
  // 若该 combo 注册在可输入区元素上，记录例外（可选增强；当前 bindRoot 全局，通常不需要）
  // 此处保留接口，实际由调用方决定是否在 input 上注册。
  return () => {
    const arr = registry.get(key);
    if (!arr) return;
    const idx = arr.indexOf(handler);
    if (idx >= 0) arr.splice(idx, 1);
    if (arr.length === 0) registry.delete(key);
  };
}

function hasRegistered(_el: HTMLElement, _combo: HotkeyCombo): boolean {
  // 当前 bindRoot 为全局根，注册均作用于全局；保留扩展位，未来支持逐元素注册。
  return false;
}

function isPassThrough(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  if (!t || !bindRoot) return false;
  if (!bindRoot.contains(t) && t !== bindRoot) return false; // 不在绑定根子树内不处理
  // 可输入区放行（除非该元素本身显式注册了 combo）
  if (ignoreInput && t.closest('input, textarea, [contenteditable="true"], [contenteditable=""]')) {
    return !hasRegistered(t, comboFromEvent(e));
  }
  // 自定义特殊 class 放行
  if (t.closest("." + ignoreClass)) return true;
  return false;
}

function dispatchKeydown(e: KeyboardEvent): void {
  // Esc 由调用方自行处理（保持原 drawer 关闭行为），此处仅处理 combo。
  if (e.key === "Escape") return;

  const combo = comboFromEvent(e);
  if (isPassThrough(e)) {
    // 忽略范围内：不 preventDefault、不处理，放行给浏览器/系统
    return;
  }

  const handlers = registry.get(comboKey(combo));
  if (handlers && handlers.length > 0) {
    e.preventDefault();
    for (const h of handlers) h(e);
  }
  // 未命中任何 combo：不 preventDefault，交给浏览器/系统
}

export const hotkeyService = {
  initHotkeyService,
  registerHotkey,
  comboFromEvent,
};

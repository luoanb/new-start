<script lang="ts">
  // PathInput：绝对路径输入 + 百度式补全下拉（当前目录直接子项 + `.` / `..`，不递归）。
  // 聚焦/输入即展示；目录不存在/无权限时逐级向父目录回退；Esc 先收下拉再请求关闭。
  import { api, c } from "$lib/api";
  import type { FsEntry } from "$lib/types";

  type SuggestItem =
    | { kind: "dot"; path: string }
    | { kind: "dotdot"; path: string }
    | { kind: "dir"; path: string }
    | { kind: "file"; path: string };

  let {
    value = $bindable(""),
    placeholder = "",
    onsubmit,
    onclose,
    class: className = "",
  }: {
    value: string;
    placeholder?: string;
    /** Enter 提交（无高亮候选时）或外部提交按钮调用 */
    onsubmit?: (v: string) => void;
    /** Esc 且下拉已收起时请求关闭宿主面板 */
    onclose?: () => void;
    class?: string;
  } = $props();

  let suggest = $state<SuggestItem[]>([]);
  let suggestOpen = $state(false);
  let active = $state(-1);
  let timer: ReturnType<typeof setTimeout> | null = null;
  let suppress = $state(false);

  /** 拆分输入为「待列出目录 + 过滤前缀」（输入以 / 结尾视为纯目录）。 */
  function splitPathInput(input: string): { parent: string; prefix: string } {
    const hasTrailing = input.endsWith("/");
    const lastSlash = input.lastIndexOf("/");
    const parent = hasTrailing
      ? input.slice(0, -1)
      : lastSlash <= 0
        ? "/"
        : input.slice(0, lastSlash);
    const prefix = hasTrailing ? "" : input.slice(lastSlash + 1);
    return { parent: parent || "/", prefix };
  }

  async function refreshSuggest(input: string) {
    const text = input.trim();
    if (!text.startsWith("/")) {
      suggest = [];
      suggestOpen = false;
      return;
    }
    const { parent } = splitPathInput(text);
    // 目标目录不存在/无权限时逐级向父目录回退，回退后过滤前缀变宽
    let entries: FsEntry[] | null = null;
    let dir = parent;
    while (dir) {
      try {
        entries = await api.call(c.fsSuggestAbs, { path: dir });
        break;
      } catch {
        if (dir === "/") break;
        dir = dir.slice(0, dir.lastIndexOf("/")) || "/";
      }
    }
    if (!entries) {
      suggest = [];
      suggestOpen = false;
      return;
    }
    // 过滤 key：text 中 dir 之后的部分，取第一段（跨层输入回退后仍能匹配）
    const rest = dir === "/" ? text.slice(1) : text.slice(dir.length + 1);
    const key = rest.split("/")[0].toLowerCase();
    const dirs = entries.filter(
      (e) => e.is_dir && e.name.toLowerCase().startsWith(key),
    );
    const files = entries.filter(
      (e) => !e.is_dir && e.name.toLowerCase().startsWith(key),
    );
    const upPath = dir === "/" ? "/" : dir.slice(0, dir.lastIndexOf("/")) || "/";
    suggest = [
      { kind: "dot", path: dir },
      { kind: "dotdot", path: upPath },
      ...dirs.map((e): SuggestItem => ({ kind: "dir", path: e.path })),
      ...files.map((e): SuggestItem => ({ kind: "file", path: e.path })),
    ];
    active = -1;
    suggestOpen = suggest.length > 0;
  }

  /** 选中候选：填入路径；目录类继续展开子项，文件类收起。 */
  function selectSuggest(item: SuggestItem) {
    suppress = true;
    value = item.path;
    suggestOpen = false;
    active = -1;
    if (item.kind === "file") return;
    void refreshSuggest(item.path + "/");
  }

  // 输入 / 外部赋值（如默认填入主目录）后 debounce 刷新；selectSuggest 已主动刷新则跳过
  $effect(() => {
    const v = value;
    if (suppress) {
      suppress = false;
      return;
    }
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void refreshSuggest(v), 120);
  });

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (suggestOpen) {
        suggestOpen = false;
        active = -1;
        e.preventDefault();
      } else {
        onclose?.();
      }
      return;
    }
    if (suggestOpen && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      const n = suggest.length;
      if (n === 0) return;
      active = e.key === "ArrowDown" ? (active + 1) % n : (active - 1 + n) % n;
      return;
    }
    if (e.key === "Enter") {
      if (suggestOpen && active >= 0 && active < suggest.length) {
        e.preventDefault();
        selectSuggest(suggest[active]);
        return;
      }
      onsubmit?.(value);
      return;
    }
    if (e.key === "Tab" && suggestOpen) {
      e.preventDefault();
      const item = suggest[active >= 0 ? active : 0];
      if (item) selectSuggest(item);
    }
  }

  function onBlur() {
    // 延迟关闭，避免点击候选（mousedown）被 blur 抢先
    setTimeout(() => {
      suggestOpen = false;
      active = -1;
    }, 150);
  }

  // 聚焦即展示当前路径的下拉
  function onFocus() {
    void refreshSuggest(value);
  }

  // 输入框自动聚焦
  function focusInput(el: HTMLInputElement) {
    el.focus();
    el.select();
  }
</script>

<div class="path-input {className}">
  <input
    class="pi-input"
    {placeholder}
    bind:value
    onfocus={onFocus}
    onkeydown={onKeydown}
    onblur={onBlur}
    use:focusInput
  />
  {#if suggestOpen}
    <div class="pi-suggest" role="listbox" aria-label="Path suggestions">
      {#each suggest as item, i (item.kind + item.path)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="pi-suggest-item"
          class:active={i === active}
          role="option"
          aria-selected={i === active}
          onmousedown={(e) => {
            e.preventDefault();
            selectSuggest(item);
          }}
        >
          <span class="pi-label" title={item.path}>{item.path}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .path-input {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .pi-input {
    width: 100%;
    padding: 2px 6px;
    font-size: var(--fs-sm);
    background: var(--color-elevated);
    color: var(--color-text);
    border: 1px solid var(--color-primary);
    border-radius: var(--radius-sm);
    outline: none;
  }
  .pi-suggest {
    position: absolute;
    top: calc(100% + 2px);
    left: 0;
    width: 220px;
    z-index: 30;
    max-height: 220px;
    overflow-y: auto;
    background: var(--color-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
    padding: 2px;
  }
  .pi-suggest-item {
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    user-select: none;
  }
  .pi-suggest-item:hover,
  .pi-suggest-item.active {
    background: var(--color-hover);
  }
  .pi-label {
    display: block;
    font-size: var(--fs-sm);
    color: var(--color-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>

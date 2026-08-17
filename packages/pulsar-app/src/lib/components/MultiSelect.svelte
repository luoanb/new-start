<script lang="ts">
  import { t } from "$lib/i18n";

  type Option = { value: string; label: string };

  let {
    value = $bindable(),
    options,
    placeholder = "",
    align = "left",
    disabled = false,
    onchange,
    class: className = "",
  }: {
    value?: string[];
    options: Option[];
    placeholder?: string;
    align?: "left" | "right";
    disabled?: boolean;
    onchange?: (values: string[]) => void;
    class?: string;
  } = $props();

  let open = $state(false);
  let highlight = $state(0);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let pos = $state<{ top: number; left: number; width: number } | null>(null);

  // $bindable prop 可能为 undefined，统一走非空视图
  const selected = $derived(value ?? []);

  const firstLabel = $derived(
    options.find((o) => o.value === selected[0])?.label ?? "",
  );
  const summary = $derived(
    selected.length === 0
      ? placeholder
      : `${t("common.selected", { count: selected.length })}${
          firstLabel ? " · " + firstLabel : ""
        }`,
  );
  const allChecked = $derived(
    options.length > 0 && selected.length === options.length,
  );

  // 把浮层 + backdrop portal 到 body，避免被 overflow:hidden / transform 祖先裁切
  function portal(node: Element) {
    document.body.appendChild(node);
    return () => {
      node.remove();
    };
  }

  // 浮层定位（相对视口，position: fixed），带视口边界钳制
  function place() {
    if (!triggerEl) return;
    const r = triggerEl.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    // 与 .dropdown max-width 保持一致，用于估算右边界
    const maxW = Math.min(300, vw - 16);
    let left = align === "right" ? r.right - r.width : r.left;
    left = Math.max(8, Math.min(left, vw - maxW - 8));
    // 估算展开高度（option 约 30px + 全选行 + 边距），底部空间不足时向上展开
    const estH = Math.min(260, options.length * 30 + 40);
    let top = r.bottom + 4;
    if (top + estH > vh - 8) top = Math.max(8, r.top - estH - 4);
    pos = { top, left, width: r.width };
  }

  function openMenu() {
    open = true;
    highlight = Math.max(
      0,
      options.findIndex((o) => o.value === selected[0]),
    );
    place();
  }

  function toggle() {
    if (disabled) return;
    if (open) close();
    else openMenu();
  }

  function close() {
    open = false;
    pos = null;
  }

  function commit(next: string[]) {
    value = next;
    onchange?.(next);
  }

  function toggleOption(v: string) {
    commit(
      selected.includes(v)
        ? selected.filter((x) => x !== v)
        : [...selected, v],
    );
  }

  function toggleAll() {
    commit(allChecked ? [] : options.map((o) => o.value));
  }

  function onTriggerKey(e: KeyboardEvent) {
    if (
      e.key === "Enter" ||
      e.key === " " ||
      e.key === "ArrowDown" ||
      e.key === "ArrowUp"
    ) {
      e.preventDefault();
      openMenu();
    }
  }

  function onMenuKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      highlight = Math.min(highlight + 1, options.length - 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      highlight = Math.max(highlight - 1, 0);
    } else if (e.key === "Home") {
      e.preventDefault();
      highlight = 0;
    } else if (e.key === "End") {
      e.preventDefault();
      highlight = options.length - 1;
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      const opt = options[highlight];
      if (opt) toggleOption(opt.value);
    }
  }

  function onWindowScrollResize() {
    if (open) place();
  }
</script>

<svelte:window onscroll={onWindowScrollResize} onresize={onWindowScrollResize} />

<div class="select {className}" class:open>
  <button
    bind:this={triggerEl}
    type="button"
    class="trigger"
    class:placeholder={selected.length === 0}
    {disabled}
    onclick={toggle}
    onkeydown={onTriggerKey}
    aria-haspopup="listbox"
    aria-expanded={open}
  >
    <span class="value">{summary}</span>
    <svg
      class="caret"
      class:flip={open}
      viewBox="0 0 24 24"
      width="14"
      height="14"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <polyline points="6 9 12 15 18 9" />
    </svg>
  </button>
</div>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    {@attach portal}
    class="backdrop"
    role="presentation"
    onclick={close}
    oncontextmenu={close}
  ></div>

  <div
    {@attach portal}
    class="dropdown {align}"
    role="listbox"
    tabindex="-1"
    style="top: {pos?.top ?? 0}px; left: {pos?.left ?? 0}px; min-width: {pos?.width ?? 0}px;"
    onkeydown={onMenuKey}
  >
    <button
      type="button"
      class="option toggle-all"
      class:checked={allChecked}
      onmouseenter={() => (highlight = -1)}
      onclick={toggleAll}
    >
      <span class="check">{allChecked ? "✓" : ""}</span>
      <span class="opt-label">{t("common.selectAll")}</span>
    </button>
    {#each options as opt, i (opt.value)}
      <button
        type="button"
        class="option"
        class:checked={selected.includes(opt.value)}
        class:highlight={i === highlight}
        role="option"
        aria-selected={selected.includes(opt.value)}
        onmouseenter={() => (highlight = i)}
        onclick={() => toggleOption(opt.value)}
      >
        <span class="check">{selected.includes(opt.value) ? "✓" : ""}</span>
        <span class="opt-label">{opt.label}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .select {
    position: relative;
    display: inline-block;
  }

  .trigger {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    width: 100%;
    min-width: 140px;
    max-width: 220px;
    padding: var(--space-1) var(--space-2);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--fs-sm);
    line-height: 1.4;
    cursor: pointer;
    transition: border-color var(--duration-fast) var(--ease-out),
                background var(--duration-fast) var(--ease-out);
  }

  .trigger:hover:not(:disabled) {
    border-color: var(--color-primary);
  }

  .trigger:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .placeholder .value {
    color: var(--color-text-muted);
  }

  .caret {
    flex-shrink: 0;
    transition: transform var(--duration-fast) var(--ease-out);
  }

  .caret.flip {
    transform: rotate(180deg);
  }

  /* 浮层与触发按钮解耦，portal 到 body，规避 overflow:hidden / transform 祖先裁切 */
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 900;
  }

  .dropdown {
    position: fixed;
    z-index: 901;
    max-width: min(300px, calc(100vw - 16px));
    max-height: 260px;
    overflow-y: auto;
    outline: none;
    background: var(--color-elevated);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.16);
  }

  .option {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-sm);
    text-align: left;
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out);
  }

  .option:hover,
  .option.highlight {
    background: var(--color-hover);
  }

  .option.checked .opt-label {
    font-weight: 600;
    color: var(--color-primary);
  }

  .check {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    line-height: 1;
    color: transparent;
  }

  .option.checked .check {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: var(--color-on-primary, #fff);
  }

  .opt-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .toggle-all {
    border-bottom: 1px solid var(--color-border);
  }
</style>

<script lang="ts">
  type Option = { value: string | number; label: string };

  let {
    value = $bindable(),
    options,
    placeholder = "",
    align = "left",
    disabled = false,
    onchange,
    class: className = "",
  }: {
    value?: string | number;
    options: Option[];
    placeholder?: string;
    align?: "left" | "right";
    disabled?: boolean;
    onchange?: (value: string | number) => void;
    class?: string;
  } = $props();

  let open = $state(false);
  let highlight = $state(0);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let floatingEl = $state<HTMLDivElement | null>(null);
  // 浮层定位（相对视口，position: fixed）
  let pos = $state<{ top: number; left: number; width: number } | null>(null);

  const selectedLabel = $derived(
    options.find((o) => o.value === value)?.label ?? placeholder,
  );

  // 把浮层 + backdrop portal 到 body，避免被 overflow:hidden / transform 祖先裁切
  function portal(node: Element) {
    document.body.appendChild(node);
    return () => {
      node.remove();
    };
  }

  function place() {
    if (!triggerEl) return;
    const r = triggerEl.getBoundingClientRect();
    pos = {
      top: r.bottom + 4,
      left: align === "right" ? r.right - r.width : r.left,
      width: r.width,
    };
  }

  function openMenu() {
    open = true;
    const idx = options.findIndex((o) => o.value === value);
    highlight = idx >= 0 ? idx : 0;
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

  function choose(v: string | number) {
    value = v;
    close();
    onchange?.(v);
  }

  function onTriggerKey(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown" || e.key === "ArrowUp") {
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
    } else if (e.key === "Enter") {
      e.preventDefault();
      const opt = options[highlight];
      if (opt) choose(opt.value);
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
    class:placeholder={selectedLabel === placeholder}
    {disabled}
    onclick={toggle}
    onkeydown={onTriggerKey}
    aria-haspopup="listbox"
    aria-expanded={open}
  >
    <span class="value">{selectedLabel}</span>
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
    bind:this={floatingEl}
    class="dropdown {align}"
    class:open
    role="listbox"
    tabindex="-1"
    style="top: {pos?.top ?? 0}px; left: {pos?.left ?? 0}px; min-width: {pos?.width ?? 0}px;"
    onkeydown={onMenuKey}
  >
    {#each options as opt, i (opt.value)}
      <button
        type="button"
        class="option"
        class:active={opt.value === value}
        class:highlight={i === highlight}
        role="option"
        aria-selected={opt.value === value}
        onmouseenter={() => (highlight = i)}
        onclick={() => choose(opt.value)}
      >
        {opt.label}
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
    min-width: 96px;
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
    /* 透明遮罩，仅用于捕获外部点击 */
  }

  .dropdown {
    position: fixed;
    z-index: 901;
    max-height: 240px;
    overflow-y: auto;
    outline: none;
    background: var(--color-elevated);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.16);
  }

  /* left / right 对齐均由 inline style 的 left + min-width 控制，无需额外规则 */

  .option {
    display: block;
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-sm);
    text-align: left;
    white-space: nowrap;
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out);
  }

  .option:hover {
    background: var(--color-hover);
  }

  .option.highlight {
    background: var(--color-hover);
  }

  .option.active {
    font-weight: 600;
    color: var(--color-primary);
  }
</style>

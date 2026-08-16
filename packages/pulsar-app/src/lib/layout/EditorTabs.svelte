<script lang="ts">
  import type { ViewMeta } from "./views";
  import { t } from "$lib/i18n";

  let {
    tabs,
    activeId,
    paneId,
    onSelect,
    onClose,
    onDrop,
    onDropToNewPane,
  }: {
    tabs: ViewMeta[];
    activeId: string | null;
    paneId: string;
    onSelect: (id: string) => void;
    onClose: (id: string) => void;
    onDrop: (panelId: string, targetPaneId: string, targetIndex: number) => void;
    onDropToNewPane: (panelId: string) => void;
  } = $props();

  // 激活视图回退：activeId 不在 tabs 中（如侧栏活动）时高亮首个主视图。
  const resolvedActive = $derived(
    tabs.some((t) => t.id === activeId) ? activeId : tabs[0]?.id,
  );

  // ── 拖拽：分栏内重排 / 跨分栏移动（HTML5 DnD）──
  // 幽灵图使用系统默认快照（WebKitGTK HiDPI 下会被放大 dpr 倍，属引擎行为，暂不处理）。
  let barEl = $state<HTMLElement | null>(null);
  let dragId = $state<string | null>(null);
  type DropState =
    | { kind: "tab"; tabId: string; before: boolean }
    | { kind: "append" };
  let dropState = $state<DropState | null>(null);
  /** 拖拽中是否悬停在「新建分栏」目标上 */
  let overNewPane = $state(false);

  function handleDragStart(e: DragEvent, id: string) {
    dragId = id;
    if (e.dataTransfer) {
      e.dataTransfer.setData("text/plain", id);
      e.dataTransfer.effectAllowed = "move";
    }
  }

  function handleDragEnd() {
    dragId = null;
    dropState = null;
    overNewPane = false;
  }

  /** 拖拽悬停「新建分栏」目标：允许放置并高亮。 */
  function handleNewPaneDragOver(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    overNewPane = true;
  }

  function handleNewPaneDragLeave(e: DragEvent) {
    const related = e.relatedTarget as Node | null;
    const el = e.currentTarget as HTMLElement | null;
    if (!el || !related || !el.contains(related)) overNewPane = false;
  }

  /** 拖放到「新建分栏」目标：把被拖面板移入新分栏（stopPropagation 防止冒泡到分栏追加）。 */
  function handleNewPaneDrop(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    overNewPane = false;
    const panelId = e.dataTransfer?.getData("text/plain") || dragId;
    if (panelId) onDropToNewPane(panelId);
  }

  /** 解析悬停目标：命中 tab 按鼠标在 tab 左/右半区给插入方向，否则视为追加到栏尾。 */
  function resolveDropTarget(e: DragEvent): DropState {
    const tabEl = (e.target as HTMLElement | null)?.closest?.(".tab");
    if (tabEl) {
      const rect = tabEl.getBoundingClientRect();
      return {
        kind: "tab",
        tabId: tabEl.getAttribute("data-id")!,
        before: e.clientX < rect.left + rect.width / 2,
      };
    }
    return { kind: "append" };
  }

  function handleBarDragOver(e: DragEvent) {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    dropState = resolveDropTarget(e);
  }

  function handleBarDragLeave(e: DragEvent) {
    const related = e.relatedTarget as Node | null;
    if (!barEl || !related || !barEl.contains(related)) dropState = null;
  }

  function handleBarDrop(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    const panelId = e.dataTransfer?.getData("text/plain") || dragId;
    const st = resolveDropTarget(e);
    if (panelId) {
      const idx =
        st.kind === "tab" ? tabs.findIndex((t) => t.id === st.tabId) : tabs.length;
      const targetIndex =
        st.kind === "tab" ? (st.before ? idx : idx + 1) : tabs.length;
      onDrop(panelId, paneId, Math.max(0, targetIndex));
    }
    dropState = null;
  }
</script>

<div class="tabs-wrap">
  <div
    class="editor-tabs"
    role="tablist"
    tabindex="0"
    bind:this={barEl}
    ondragover={handleBarDragOver}
    ondragleave={handleBarDragLeave}
    ondrop={handleBarDrop}
  >
    {#each tabs as tab}
      <button
        class="tab"
        class:active={tab.id === resolvedActive}
        class:drop-before={dropState?.kind === "tab" && dropState.tabId === tab.id && dropState.before}
        class:drop-after={dropState?.kind === "tab" && dropState.tabId === tab.id && !dropState.before}
        draggable="true"
        data-id={tab.id}
        ondragstart={(e) => handleDragStart(e, tab.id)}
        ondragend={handleDragEnd}
        onclick={() => onSelect(tab.id)}
      >
        {#if tab.icon}<span class="icon">{@html tab.icon}</span>{/if}
        <span class="label">{t(tab.label)}</span>
        <span
          class="close"
          role="button"
          tabindex="-1"
          title="Close"
          onclick={(e) => { e.stopPropagation(); onClose(tab.id); }}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              e.stopPropagation();
              onClose(tab.id);
            }
          }}
        >✕</span>
      </button>
    {/each}
    <div class="tabs-spacer" class:drop-append={dropState?.kind === "append"}></div>
  </div>

  {#if dragId}
    <!-- 拖拽中右侧悬浮的「新建分栏」目标：拖入即把面板移入新分栏 -->
    <div
      class="new-pane-drop"
      class:over={overNewPane}
      role="presentation"
      ondragover={handleNewPaneDragOver}
      ondragleave={handleNewPaneDragLeave}
      ondrop={handleNewPaneDrop}
    >
      <span class="new-pane-icon">＋</span>
      <span>{t("common.newPane")}</span>
    </div>
  {/if}
</div>

<style>
  .tabs-wrap {
    position: relative;
    flex-shrink: 0;
  }

  .editor-tabs {
    display: flex;
    align-items: flex-end;
    flex-shrink: 0;
    height: 32px;
    background: var(--color-bg);
    border-bottom: var(--border-width) solid var(--color-border);
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .editor-tabs::-webkit-scrollbar { display: none; }

  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: 32px;
    padding: 0 var(--space-2);
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: var(--fs-sm);
    color: var(--color-text-muted);
    border-right: var(--border-width) solid var(--color-border);
    border-top: 2px solid transparent;
    transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out);
    white-space: nowrap;
  }

  .tab:hover { background: var(--color-hover); color: var(--color-text); }

  .tab.active {
    background: var(--color-surface);
    color: var(--color-text);
    border-top-color: var(--color-primary);
  }

  .icon { display: inline-flex; align-items: center; font-size: 12px; line-height: 1; }
  .label { font-size: var(--fs-xs); font-weight: 500; }

  .close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    font-size: 10px;
    border-radius: var(--radius-sm);
    color: inherit;
    opacity: 0;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .tab:hover .close { opacity: 0.7; }
  .tab.active .close { opacity: 0.6; }
  .close:hover { opacity: 1 !important; background: var(--color-hover); }

  .tabs-spacer { flex: 1; height: 100%; position: relative; }

  /* 拖拽中右侧悬浮的「新建分栏」目标 */
  .new-pane-drop {
    position: absolute;
    right: var(--space-2);
    top: 3px;
    bottom: 3px;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-2);
    font-size: var(--fs-xs);
    font-weight: 500;
    color: var(--color-text-muted);
    border: 1px dashed var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    cursor: copy;
    user-select: none;
    pointer-events: auto;
    z-index: 5;
  }
  .new-pane-icon { font-size: 11px; line-height: 1; }
  .new-pane-drop.over {
    color: var(--color-on-primary);
    background: var(--color-primary);
    border-color: transparent;
  }

  /* 拖拽落点指示：目标 tab 左/右侧或栏尾竖线 */
  .tab.drop-before::before,
  .tab.drop-after::after,
  .tabs-spacer.drop-append::before {
    content: "";
    position: absolute;
    top: 4px;
    bottom: 4px;
    width: 2px;
    border-radius: 1px;
    background: var(--color-primary);
    pointer-events: none;
  }
  .tab.drop-before::before { left: 0; }
  .tab.drop-after::after { right: 0; }
  .tabs-spacer.drop-append::before { left: 0; }
</style>

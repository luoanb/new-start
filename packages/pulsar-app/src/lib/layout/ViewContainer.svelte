<script lang="ts">
  import type { ViewContainerId, ViewRegistration } from "./views";
  import { viewRegistry, canMoveTo } from "./views";
  import { useViewContext } from "./viewContext";
  import ViewHost from "./ViewHost.svelte";
  import { t } from "$lib/i18n";

  let {
    containerId,
  }: {
    containerId: ViewContainerId;
  } = $props();

  const ctx = useViewContext();
  const layout = ctx.stores.layout;

  /** 容器内可见视图（按持久化顺序）。 */
  const registrations = $derived(
    layout.state.containers[containerId].views
      .map((id) => viewRegistry[id])
      .filter((r): r is ViewRegistration => !!r),
  );

  /** 当前激活视图（状态失效时回退到首个可见视图）。 */
  const activeView = $derived(
    registrations.find((r) => r.id === layout.state.containers[containerId].activeView) ??
      registrations[0],
  );

  /** 可被拖入本容器的全部视图（⋯ 菜单用）。 */
  const movableViews = $derived(
    Object.values(viewRegistry).filter((r) => canMoveTo(r.id, containerId)),
  );

  const CONTAINER_LABELS: Record<ViewContainerId, string> = {
    sidebar: "Sidebar",
    info: "Info",
    panel: "Panel",
  };

  function visibleInAnyContainer(viewId: string): boolean {
    return Object.values(layout.state.containers).some((c) => c.views.includes(viewId));
  }

  function locationOf(viewId: string): string {
    for (const cid of Object.keys(layout.state.containers) as ViewContainerId[]) {
      if (layout.state.containers[cid].views.includes(viewId)) return CONTAINER_LABELS[cid];
    }
    return "Hidden";
  }

  // ── ⋯ 菜单（视图显隐）──
  let menuOpen = $state(false);

  function toggleViewVisibility(viewId: string) {
    if (visibleInAnyContainer(viewId)) layout.hideView(viewId);
    else layout.showView(viewId, containerId);
  }

  // ── 拖拽换容器（Pointer Events）──
  // 跨平台可靠（WebKitGTK 的 HTML5 DnD 支持不稳定）。状态留在"源实例"的闭包里：
  // 拖拽全程监听 window 的 pointermove/pointerup，释放时用命中测试找目标容器并 moveView，
  // 因此目标容器无需读取源实例状态，天然支持跨区域。
  let dragViewId: string | null = null; // 正在拖拽的视图
  let dragPointerId = -1;
  let dragStartX = 0;
  let dragStartY = 0;
  let dragging = false; // 是否已超过阈值进入拖拽态（否则视为普通点击）
  /** 同容器内重排落点：悬停 tab + 插入方向（before = 左侧半区）。 */
  let dropTarget = $state<{ tabId: string; before: boolean } | null>(null);
  // 拖拽预览：用 $state + Svelte 模板渲染（同 Select.svelte 的浮层模式，该环境已验证可靠），
  // 而非动态 DOM 上手动改 style。
  let preview = $state<{ title: string; x: number; y: number } | null>(null);

  function handleTabPointerDown(e: PointerEvent, viewId: string) {
    if (e.button !== 0) return;
    dragPointerId = e.pointerId;
    dragStartX = e.clientX;
    dragStartY = e.clientY;
    dragViewId = viewId;
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp, { once: true });
    window.addEventListener("pointercancel", handlePointerCancel, { once: true });
  }

  function handlePointerMove(e: PointerEvent) {
    if (e.pointerId !== dragPointerId) return;
    if (!dragging) {
      const dist = Math.abs(e.clientX - dragStartX) + Math.abs(e.clientY - dragStartY);
      if (dist < 6) return; // 未达拖拽阈值，保持为点击
      dragging = true;
      createPreview();
    }
    positionPreview(e.clientX, e.clientY);
    updateTarget(e.clientX, e.clientY);
    updateDropTarget(e.clientX, e.clientY);
  }

  function handlePointerUp(e: PointerEvent) {
    if (e.pointerId !== dragPointerId) return;
    const target = containerAt(e.clientX, e.clientY);
    const drop = dropTarget;
    cleanupDrag();
    if (dragging && dragViewId && target && canMoveTo(dragViewId, target)) {
      if (drop && target === containerId && drop.tabId !== dragViewId) {
        // 同容器内重排：按落点计算插入索引（基于移动前顺序）
        const views = layout.state.containers[target].views;
        const idx = views.findIndex((v) => v === drop.tabId);
        layout.reorderView(target, dragViewId, drop.before ? idx : idx + 1);
      } else {
        layout.moveView(dragViewId, target);
      }
    }
    dragViewId = null;
    dragging = false;
  }

  function handlePointerCancel() {
    cleanupDrag();
    dragViewId = null;
    dragging = false;
  }

  function cleanupDrag() {
    window.removeEventListener("pointermove", handlePointerMove);
    window.removeEventListener("pointerup", handlePointerUp);
    window.removeEventListener("pointercancel", handlePointerCancel);
    preview = null;
    dropTarget = null;
    document.querySelectorAll(".view-container.dragging-target").forEach((el) =>
      el.classList.remove("dragging-target"),
    );
  }

  /** 命中测试：从坐标向上找最近的视图容器（根元素带 data-container）。 */
  function containerAt(x: number, y: number): ViewContainerId | null {
    let node: Element | null = document.elementFromPoint(x, y);
    while (node) {
      const cid = node.getAttribute("data-container");
      if (cid === "sidebar" || cid === "info" || cid === "panel") return cid;
      node = node.parentElement;
    }
    return null;
  }

  /** 拖拽中高亮可投放的目标容器。 */
  function updateTarget(x: number, y: number) {
    document.querySelectorAll(".view-container.dragging-target").forEach((el) =>
      el.classList.remove("dragging-target"),
    );
    const target = containerAt(x, y);
    if (target && dragViewId && canMoveTo(dragViewId, target)) {
      document
        .querySelector(`.view-container[data-container="${target}"]`)
        ?.classList.add("dragging-target");
    }
  }

  /** 解析同容器内拖拽落点：命中本容器 tab 左/右半区决定插入方向。 */
  function updateDropTarget(x: number, y: number) {
    const el = document.elementFromPoint(x, y) as HTMLElement | null;
    const tabEl = el?.closest?.(".tab[data-view-id]") as HTMLElement | null;
    const tabId = tabEl?.getAttribute("data-view-id") ?? null;
    if (!tabEl || !tabId || !registrations.some((r) => r.id === tabId)) {
      dropTarget = null;
      return;
    }
    const rect = tabEl.getBoundingClientRect();
    dropTarget = { tabId, before: x < rect.left + rect.width / 2 };
  }

  function createPreview() {
    if (!dragViewId) return;
    preview = { title: viewRegistry[dragViewId]?.title ?? dragViewId, x: 0, y: 0 };
  }

  function positionPreview(x: number, y: number) {
    if (preview) {
      preview.x = x;
      preview.y = y;
    }
  }

  /** 浮层 portal 到 body，避免被容器 overflow 裁切（与 Select.svelte 一致）。 */
  function portal(node: Element) {
    document.body.appendChild(node);
    return {
      destroy() {
        node.remove();
      },
    };
  }
</script>

<div
  class="view-container {containerId}"
  data-container={containerId}
  role="region"
  aria-label={CONTAINER_LABELS[containerId]}
>
  <header class="container-header">
    <div
      class="container-tabs"
      onwheel={(e) => {
        // hover 时滚轮 → 左右滚动 tab 栏（阻止页面纵向滚动），与主区域 EditorTabs 一致
        e.preventDefault();
        e.currentTarget.scrollLeft += e.deltaY + e.deltaX;
      }}
    >
      {#if registrations.length > 1}
        {#each registrations as reg (reg.id)}
          <button
            class="tab"
            class:active={activeView?.id === reg.id}
            class:drop-before={dropTarget?.tabId === reg.id && dropTarget.before}
            class:drop-after={dropTarget?.tabId === reg.id && !dropTarget.before}
            data-view-id={reg.id}
            onpointerdown={(e) => handleTabPointerDown(e, reg.id)}
            onclick={() => layout.setContainerView(containerId, reg.id)}
          >
            {#if reg.icon}<span class="tab-icon">{@html reg.icon}</span>{/if}
            <span class="tab-label">{t(reg.title)}</span>
          </button>
        {/each}
      {:else if activeView}
        <!-- 单视图：标题区亦可拖拽（拖当前激活视图） -->
        <button
          class="tab title-tab"
          class:active={true}
          onpointerdown={(e) => handleTabPointerDown(e, activeView.id)}
        >
          {#if activeView.icon}<span class="tab-icon">{@html activeView.icon}</span>{/if}
          <span class="tab-label">{t(activeView.title)}</span>
        </button>
      {/if}
    </div>
    <div class="header-actions">
      <button
        class="icon-btn"
        class:active={menuOpen}
        onclick={() => (menuOpen = !menuOpen)}
        aria-label="View actions"
        title="View actions"
      >⋯</button>
    </div>
  </header>

  <div class="container-body">
    {#if activeView}
      <ViewHost registration={activeView} />
    {:else}
      <p class="empty">No view</p>
    {/if}
  </div>

  {#if menuOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="menu-backdrop" role="presentation" onclick={() => (menuOpen = false)}></div>
    <div class="views-menu" role="menu" aria-label="View actions">
      <div class="menu-title">Views</div>
      {#each movableViews as reg (reg.id)}
        {@const visible = visibleInAnyContainer(reg.id)}
        <label class="menu-item">
          <input type="checkbox" checked={visible} onchange={() => toggleViewVisibility(reg.id)} />
          <span class="menu-label">{t(reg.title)}</span>
          <span class="menu-loc">{locationOf(reg.id)}</span>
        </label>
      {/each}
    </div>
  {/if}

  {#if preview}
    <div
      class="view-drag-preview"
      use:portal
      style="position: fixed; z-index: 500; pointer-events: none; left: {preview.x + 12}px; top: {preview.y + 12}px"
    >{t(preview.title)}</div>
  {/if}
</div>

<style>
  .view-container {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
    height: 100%;
    background: var(--color-surface);
  }

  /* 拖拽目标高亮/浮动预览通过 JS 动态加 class 或 appendChild，须为全局选择器 */
  :global(.view-container.dragging-target) {
    outline: 2px dashed var(--color-primary);
    outline-offset: -2px;
  }

  :global(.view-drag-preview) {
    position: fixed;
    width:120px;
    height:24px;
    z-index: 500;
    padding: 2px 10px;
    background: var(--color-elevated);
    border: 1px solid var(--color-primary);
    border-radius: 999px; /* 小标题栏 chip，示意被拖视图即可 */
    font-size: var(--fs-xs);
    line-height: 1.7;
    color: var(--color-primary);
    pointer-events: none;
    white-space: nowrap;
    opacity: 0.95;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
  }

  .container-header {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    height: 32px;
    padding: 0 var(--space-1);
    border-bottom: var(--border-width) solid var(--color-border);
    background: var(--color-surface);
  }

  .container-tabs {
    display: flex;
    align-items: center;
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .container-tabs::-webkit-scrollbar { display: none; }

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
    font-weight: 500;
    color: var(--color-text-muted);
    border-bottom: 2px solid transparent;
    white-space: nowrap;
    user-select: none;
    -webkit-user-select: none;
    touch-action: none; /* 拖动时避免触发滚动 */
    transition: color var(--duration-fast) var(--ease-out);
  }
  .tab:hover { color: var(--color-text); }
  .tab.active { color: var(--color-primary); border-bottom-color: var(--color-primary); }
  .title-tab { text-transform: uppercase; letter-spacing: 0.02em; font-weight: 600; cursor: grab; }

  /* 同容器重排拖拽落点指示：目标 tab 左/右侧竖线 */
  .tab.drop-before::before,
  .tab.drop-after::after {
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

  .tab-icon { display: inline-flex; align-items: center; font-size: 12px; pointer-events: none; }
  .tab-label { font-size: var(--fs-xs); }

  .header-actions {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    padding-left: var(--space-1);
  }
  .icon-btn {
    border: none;
    background: transparent;
    font-size: var(--fs-base);
    line-height: 1;
    padding: 2px 6px;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
  }
  .icon-btn:hover, .icon-btn.active { background: var(--color-hover); color: var(--color-text); }

  .container-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .empty {
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--fs-sm);
    padding: var(--space-4) 0;
  }

  /* ── ⋯ 菜单 ── */
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    background: transparent;
  }
  .views-menu {
    position: absolute;
    top: 32px;
    right: 6px;
    z-index: 100;
    min-width: 180px;
    background: var(--color-elevated);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
    padding: var(--space-1);
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .menu-title {
    padding: var(--space-1) var(--space-2);
    font-size: var(--fs-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }
  .menu-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: var(--fs-sm);
    color: var(--color-text);
  }
  .menu-item:hover { background: var(--color-hover); }
  .menu-item input[type="checkbox"] { accent-color: var(--color-primary); }
  .menu-label { flex: 1; }
  .menu-loc { font-size: var(--fs-xs); color: var(--color-text-muted); }
</style>

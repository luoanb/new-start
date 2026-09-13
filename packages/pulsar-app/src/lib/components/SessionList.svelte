<script lang="ts">
  import { tick } from "svelte";
  import { t } from "$lib/i18n";
  import { useViewContext } from "$lib/layout/viewContext";
  import type { ConversationSummary } from "$lib/types";
  import { CopyToClipboard } from "$lib/utils";
  import { SESSION_MODES, findSessionMode } from "$lib/sessionModes";

  // 数据/命令统一来自 ViewContext；collapsed 是纯视觉 prop（窄侧栏形态）。
  const ctx = useViewContext();
  let { collapsed = false }: { collapsed?: boolean } = $props();

  let activeId = $derived(ctx.stores.data.state.activeConversationId ?? "");
  let conversations = $derived(ctx.stores.data.state.conversations);
  let hasMore = $derived(ctx.stores.data.state.conversationsHasMore);
  let loadingMore = $derived(ctx.stores.data.state.conversationsLoadingMore);
  let runningSessionIds = $derived(
    new Set(ctx.stores.data.state.runningSessions.map((s) => s.session_id)),
  );

  // 侧栏滚动容器（滚动近底部时追加下一页会话）。
  let listEl: HTMLDivElement | undefined = $state();

  function handleScroll() {
    const el = listEl;
    if (!el) return;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 40) {
      void ctx.stores.data.loadMoreConversations();
    }
  }

  // 复制反馈：记录最近复制成功的会话 id，短暂显示「已复制」。
  let copiedId = $state<string | null>(null);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;

  async function copyId(id: string) {
    await CopyToClipboard.copyText(id);
    copiedId = id;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => (copiedId = null), 1500);
  }

  const onSelect = (id: string) => ctx.commands.selectConversation(id);
  const onCreate = () => ctx.commands.openCreateModal();
  const onClose = (id: string) => void ctx.commands.closeSession(id);
  const onToggle = () => ctx.stores.layout.toggleSidebar();

  // ── 新建会话入口：会话模式下拉 + 一键新建（组合按钮）──
  // 选中模式与会话面板/顶栏共享（ViewContext.ui.sessionMode，组合根持有），任一处改动即时同步；
  // 模式清单来自 $lib/sessionModes（与 SessionCreateModal 同一份定义）。
  let modeMenuOpen = $state(false);
  let comboEl: HTMLDivElement | undefined = $state();
  let menuEl: HTMLDivElement | undefined = $state();
  let menuPos = $state<{ top: number; left: number } | null>(null);
  // 缩放动画原点（如 "top left" / "top right" / "bottom right"），随展开方向与水平对齐方式变化。
  let menuOrigin = $state("top left");

  const currentMode = $derived(ctx.ui.sessionMode);
  const currentModeLabel = $derived(t(findSessionMode(currentMode)?.labelKey ?? currentMode));

  // 浮层 portal 到 body：.sidebar 为 overflow:hidden，内联绝对定位会被裁切。
  function portal(node: Element) {
    document.body.appendChild(node);
    return () => {
      node.remove();
    };
  }

  /**
   * 定位下拉浮层（锚点 = 触发按钮的右下角）：
   * - 水平：**恒右对齐**——触发器在标题栏里就是靠右的，菜单贴其右缘向左展开。
   * - 垂直：**优先向下**；下方空间不足才向上翻转（翻转后锚点变为右上角）。
   * 最后统一夹进视口（两侧各留 8px）兜底。
   * 宽度随内容而定，故先给初值，`tick()` 后用实测宽度二次校正（同帧完成，无可见跳动）。
   */
  async function placeMenu() {
    const el = comboEl;
    if (!el) return;
    const r = el.getBoundingClientRect();
    // 粗略估计面板高（每项两行 ≈ 50px），仅用于判断是否需要向上展开。
    const estH = SESSION_MODES.length * 50 + 12;
    const below = r.bottom + 4;
    const flipUp = below + estH > window.innerHeight - 8;
    const top = flipUp ? Math.max(8, r.top - estH - 4) : below;
    menuPos = { top, left: Math.max(8, r.left) };

    await tick();
    const menu = menuEl;
    if (!menu) return;
    const vw = window.innerWidth;
    // 用 offsetWidth 而非 getBoundingClientRect().width：后者会被入场动画的 scale(0.95) 缩水，
    // 导致右对齐时右缘外溢（实测偏 ~10px）。
    const w = menu.offsetWidth;
    const left = Math.max(8, Math.min(r.right - w, vw - 8 - w));
    menuPos = { top, left };
    menuOrigin = `${flipUp ? "bottom" : "top"} right`;
  }

  function closeModeMenu() {
    modeMenuOpen = false;
    menuPos = null;
  }

  function toggleModeMenu() {
    if (modeMenuOpen) {
      closeModeMenu();
    } else {
      modeMenuOpen = true;
      void placeMenu();
    }
  }

  /** 组合按钮「+」：直接用当前选中模式新建会话。 */
  function createWithCurrentMode() {
    void ctx.commands.createSession(ctx.ui.sessionMode);
  }

  /** 下拉选中：先更新共享选择（顶栏同源同步），再创建该类型的新会话。 */
  function chooseMode(modeId: string) {
    ctx.ui.sessionMode = modeId;
    closeModeMenu();
    void ctx.commands.createSession(modeId);
  }

  function onMenuKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      closeModeMenu();
    }
  }

  /**
   * 在 main 区打开绑定该会话的窗口（panel.id = `chat:${id}`，固定绑定，不可切换）。
   * 插入当前激活分栏（作为新 tab）；无激活分栏时才新开一栏。
   * 首次打开先加载该会话消息视图；已存在同会话窗口则直接激活，不重拉（避免打断滚动位置）。
   */
  const onOpenWindow = (id: string) => {
    const instanceId = `chat:${id}`;
    const layout = ctx.stores.layout;
    const exists = layout.state.main.panes.some((p) =>
      p.panels.some((x) => x.id === instanceId)
    );
    if (!exists) void ctx.stores.data.refreshMessages(id);
    const activeIdx = layout.state.main.panes.findIndex(
      (p) => p.id === layout.state.main.activePaneId,
    );
    layout.insertPanel("chat", activeIdx >= 0 ? activeIdx : "new", instanceId);
  };

  const modeLabel: Record<string, string> = {
    chat: "Chat",
    agent: "Agent",
    assistant: "Assistant",
    system: "System",
  };

  function formatTime(ts: number): string {
    const d = new Date(ts);
    const now = new Date();
    const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    const dayMs = 86_400_000;
    if (ts >= startOfToday) {
      const h = d.getHours().toString().padStart(2, "0");
      const m = d.getMinutes().toString().padStart(2, "0");
      return `${h}:${m}`;
    }
    if (ts >= startOfToday - dayMs) return t("sessionList.yesterday");
    if (d.getFullYear() === now.getFullYear()) return `${d.getMonth() + 1}/${d.getDate()}`;
    return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()}`;
  }

  // 会话标题：取首条 user/assistant 文本消息摘要（后端 summary.preview），无则显示占位。
  function sessionTitle(conv: ConversationSummary): string {
    return conv.preview?.trim() || t("sessionList.newSession");
  }
</script>

<svelte:window onresize={() => modeMenuOpen && void placeMenu()} />

<aside class="sidebar" class:collapsed>
  <div class="sidebar-header">
    {#if !collapsed}
      <h2>{t("sessionList.title")}</h2>
      <div class="header-actions">
        <!-- 新建会话：常规 Split Button（对齐 MUI / WinUI / Fluent）——
             左「主段」= 默认动作（用当前模式新建，图标 + 当前模式名）；右「caret 段」= 独立小段，展开模式菜单。
             caret 归主按钮（表示「更多同类动作」），不再挂在模式标签内；菜单项选定后按该模式新建。 -->
        <div class="create-combo" class:open={modeMenuOpen} bind:this={comboEl}>
          <button
            class="combo-part combo-primary"
            onclick={createWithCurrentMode}
            title={t("sessionList.newButton")}
            aria-label={t("sessionList.newButton")}
          >
            <!-- 新建动作图标：标准「加号」（与全局 add 图标一致），比「气泡+加号」在 14px 下更干净、语义更直接 -->
            <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
            <span class="combo-mode-label">{currentModeLabel}</span>
          </button>
          <span class="combo-sep" aria-hidden="true"></span>
          <button
            class="combo-part combo-caret"
            onclick={toggleModeMenu}
            title={t("createModal.title")}
            aria-label={t("createModal.title")}
            aria-haspopup="listbox"
            aria-expanded={modeMenuOpen}
          >
            <svg class="combo-caret-ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="6 9 12 15 18 9"/></svg>
          </button>
        </div>
        <button class="icon-btn" onclick={onToggle} title={t("sessionList.collapseSidebar")}>
          <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
        </button>
      </div>
    {:else}
      <button class="icon-btn expand-btn" onclick={onToggle} title={t("sessionList.expandSidebar")}>
        <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
      </button>
      <button class="icon-btn" onclick={createWithCurrentMode} title={t("sessionList.newButton")}>
        <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
      </button>
    {/if}
  </div>

  {#if !collapsed && modeMenuOpen}
    <!-- 模式下拉：浮层与透明 backdrop 均 portal 到 body（规避 .sidebar overflow:hidden 裁切） -->
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div
      {@attach portal}
      class="mode-backdrop"
      role="presentation"
      onclick={closeModeMenu}
      oncontextmenu={closeModeMenu}
    ></div>
    <div
      {@attach portal}
      bind:this={menuEl}
      class="mode-menu"
      role="listbox"
      tabindex="-1"
      style="top: {menuPos?.top ?? 0}px; left: {menuPos?.left ?? 0}px; transform-origin: {menuOrigin};"
      onkeydown={onMenuKey}
    >
      {#each SESSION_MODES as mode (mode.id)}
        <button
          type="button"
          class="mode-option"
          class:active={mode.id === currentMode}
          role="option"
          aria-selected={mode.id === currentMode}
          onclick={() => chooseMode(mode.id)}
        >
          <svg class="mode-option-check" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12"/></svg>
          <span class="mode-option-text">
            <span class="mode-option-label">{t(mode.labelKey)}</span>
            <span class="mode-option-desc">{t(mode.descKey)}</span>
          </span>
        </button>
      {/each}
    </div>
  {/if}

  {#if !collapsed}
    <div class="session-list" bind:this={listEl} onscroll={handleScroll}>
      {#if conversations.length === 0}
        <div class="empty">
          <svg class="empty-icon" viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5Z" />
          </svg>
          <p>{t("sessionList.emptyHint")}</p>
          <button class="btn btn-primary" onclick={onCreate}>{t("sessionList.newButton")}</button>
        </div>
      {:else}
        {#each conversations as conv}
          <button
            class="session-item"
            class:active={conv.id === activeId}
            onclick={() => onSelect(conv.id)}
          >
            <div class="session-info">
              <span class="session-title" title={sessionTitle(conv)}>
                <span class="session-title-text">{sessionTitle(conv)}</span>
              </span>
              <span class="session-meta">
                {#if runningSessionIds.has(conv.id)}
                  <span class="running-badge" title={t("sessionList.running")}>{t("sessionList.running")}</span>
                {/if}
                <span class="mode-badge {conv.mode}">{modeLabel[conv.mode] ?? conv.mode}</span>
                <span class="session-count">{conv.message_count} {t("sessionList.msgs")}</span>
                <span class="session-time" title={new Date(conv.updated_at).toLocaleString()}>
                  {formatTime(conv.updated_at)}
                </span>
              </span>
            </div>
            <div class="session-actions">
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <span
                class="open-btn"
                role="button"
                tabindex="-1"
                onclick={(e) => { e.stopPropagation(); onOpenWindow(conv.id); }}
                title={t("sessionList.openWindow")}
              >
                <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/></svg>
              </span>
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <span
                class="copy-btn"
                class:copied={copiedId === conv.id}
                role="button"
                tabindex="-1"
                onclick={(e) => { e.stopPropagation(); void copyId(conv.id); }}
                title={copiedId === conv.id ? t("chatMessage.copied") : t("sessionList.copyId")}
              >
                {#if copiedId === conv.id}
                  <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12"/></svg>
                {:else}
                  <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
                {/if}
              </span>
              {#if runningSessionIds.has(conv.id)}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <span
                  class="close-btn"
                  role="button"
                  tabindex="-1"
                  onclick={(e) => { e.stopPropagation(); onClose(conv.id); }}
                  title={t("sessionList.closeSession")}
                >
                  <svg viewBox="0 0 24 24" width="10" height="10" fill="currentColor" aria-hidden="true"><rect x="6" y="6" width="12" height="12" rx="1.5"/></svg>
                </span>
              {/if}
            </div>
          </button>
        {/each}
        {#if loadingMore}
          <div class="loading-more">{t("sessionList.loadingMore")}</div>
        {/if}
      {/if}
    </div>
  {/if}
</aside>

<style>
  .sidebar { display: flex; flex-direction: column; background: var(--color-surface); border-right: var(--border-width) solid var(--color-border); width: 100%; height: 100%; overflow: hidden; z-index: 1; box-shadow: 2px 0 8px rgba(0,0,0,0.05); }
  .sidebar.collapsed { width: 48px; }
  .sidebar-header { display: flex; align-items: center; justify-content: space-between; padding: var(--space-2) var(--space-3); border-bottom: var(--border-width) solid var(--color-border); min-height: 48px; }
  .sidebar.collapsed .sidebar-header { flex-direction: column; gap: var(--space-2); padding: var(--space-2); }
  .sidebar-header h2 { margin: 0; font-size: var(--fs-sm); font-weight: 600; }
  .header-actions { display: flex; gap: var(--space-1); }
  .icon-btn { background: none; border: none; border-radius: var(--radius-sm); cursor: pointer; width: 26px; height: 26px; display: inline-flex; align-items: center; justify-content: center; font-size: var(--fs-base); color: var(--color-text-muted); transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out); }
  .icon-btn:hover { background: var(--color-hover); color: var(--color-text); }
  .ic { width: 14px; height: 14px; }
  /* ── 新建会话 Split Button（对齐 MUI / WinUI / Fluent 的 split button）──
     左「主段」= 默认动作（图标 + 当前模式名，点击即按当前模式新建）；
     右「caret 段」= 独立小段（仅 chevron，hover 高亮、aria-expanded 联动旋转），点击展开模式菜单。
     Ghost 样式：无边框无底色，与相邻 icon-btn 一致；两段之间满高 1px 细分隔线（align-self: stretch）。 */
  .create-combo { display: inline-flex; align-items: center; }
  .combo-part { display: inline-flex; align-items: center; justify-content: center; gap: var(--space-1); height: 26px; padding: 0 var(--space-1); border: none; border-radius: var(--radius-sm); background: transparent; color: var(--color-text-muted); font-size: var(--fs-sm); cursor: pointer; transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out); }
  .combo-part:hover { background: var(--color-hover); color: var(--color-text); }
  .combo-part:focus-visible { outline: 2px solid var(--color-primary); outline-offset: -1px; }
  /* 主段：左外圆角、右直角（与 caret 段拼合）；label 过宽时省略号收敛。 */
  .combo-primary { padding: 0 var(--space-2); border-radius: var(--radius-sm) 0 0 var(--radius-sm); max-width: 120px; }
  .combo-mode-label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* 分隔线：width 1px + align-self: stretch 撑满 26px 行高，与两侧段落等高。 */
  .combo-sep { flex-shrink: 0; align-self: stretch; width: 1px; background: var(--color-border); }
  /* caret 段：独立小段（比主段窄，对齐 MUI split button 的 size="small" 视觉），右外圆角、左直角。 */
  .combo-caret { width: 22px; padding: 0; border-radius: 0 var(--radius-sm) var(--radius-sm) 0; }
  .combo-caret-ic { width: 12px; height: 12px; flex-shrink: 0; opacity: 0.7; transition: transform var(--duration-fast) var(--ease-out); }
  .create-combo.open .combo-caret-ic { transform: rotate(180deg); }
  /* 模式下拉：轻量菜单（两行项：模式名 + 说明，左侧勾；4px 内边距 + 细边框 + 柔和阴影）。
     浮层 portal 到 body，position: fixed 由内联 style 注入。 */
  .mode-backdrop { position: fixed; inset: 0; z-index: 900; }
  /* 面板：宽度随内容自适应（max-content + max/min 夹取）、高度随内容且带安全上限。
     portal 到 body 的浮层已不再受 app.html 的全局 inset 规则影响（该规则已收窄到 `#app-shell`），
     故本面板按普通 `position: fixed` + 内联 top/left 定位、宽高随内容。
     阴影/圆角/内边距对齐 shadcn DropdownMenuContent（p-1 / rounded-md / shadow-md）。 */
  .mode-menu { position: fixed; z-index: 901; width: max-content; min-width: 208px; max-width: min(280px, calc(100vw - 16px)); max-height: min(320px, calc(100vh - 24px)); overflow-y: auto; overscroll-behavior: contain; padding: 4px; border: var(--border-width) solid var(--color-border); border-radius: var(--radius-md); background: var(--color-elevated); box-shadow: 0 6px 16px -4px rgba(0, 0, 0, 0.16), 0 2px 6px -2px rgba(0, 0, 0, 0.12); outline: none; animation: mode-menu-in var(--duration-fast) var(--ease-out); }
  @keyframes mode-menu-in { from { opacity: 0; transform: translateY(-4px) scale(0.95); } to { opacity: 1; transform: none; } }
  /* 行：左侧固定「勾选槽」+ 右侧两行文本（模式名 + 灰色说明，对齐 VS Code / macOS 菜单与 shadcn item-with-description）；
     勾选槽恒定占位（未选中 opacity:0），所有行左缘对齐。勾选顶部对齐首行文字。 */
  .mode-option { display: flex; align-items: flex-start; gap: var(--space-2); width: 100%; padding: 6px var(--space-2); border: none; border-radius: var(--radius-sm); background: transparent; color: var(--color-text); font-size: var(--fs-sm); text-align: left; cursor: pointer; transition: background var(--duration-fast) var(--ease-out); }
  .mode-option:hover { background: var(--color-hover); }
  .mode-option-text { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
  .mode-option-label { font-size: var(--fs-sm); color: var(--color-text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .mode-option-desc { font-size: var(--fs-xs); color: var(--color-text-muted); line-height: 1.35; white-space: normal; }
  .mode-option-check { flex-shrink: 0; width: 14px; height: 14px; margin-top: 1px; color: var(--color-primary); opacity: 0; }
  .mode-option.active .mode-option-check { opacity: 1; }
  .session-list { flex: 1; overflow-y: auto; padding: 0; }
  .loading-more { padding: var(--space-2); text-align: center; font-size: var(--fs-xs); color: var(--color-text-muted); }
  .empty { display: flex; flex-direction: column; align-items: center; gap: var(--space-2); text-align: center; padding: var(--space-8) var(--space-3); color: var(--color-text-muted); font-size: var(--fs-xs); }
  .empty-icon { opacity: 0.5; }
  .session-item { display: flex; align-items: center; gap: var(--space-2); width: 100%; padding: var(--space-2) var(--space-2); border-radius: var(--radius-sm); border: none; background: transparent; cursor: pointer; text-align: left; transition: background var(--duration-fast) var(--ease-out); color: var(--color-text); }
  .session-item:hover { background: var(--color-hover); }
  .session-item.active { background: color-mix(in oklch, var(--color-primary) 14%, transparent); }
  .session-info { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .session-title { display: flex; align-items: center; gap: var(--space-1); font-size: var(--fs-sm); min-width: 0; }
  .session-title-text { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .running-badge { flex-shrink: 0; font-size: var(--fs-xs); color: var(--color-success); }
  .session-meta { display: flex; align-items: center; gap: var(--space-2); font-size: var(--fs-xs); color: var(--color-text-muted); white-space: nowrap; overflow: hidden; min-width: 0; }
  .mode-badge { font-size: var(--fs-xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.03em; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--color-text-muted); }
  .mode-badge.chat { color: var(--color-primary); }
  .mode-badge.agent { color: var(--color-success); }
  .mode-badge.assistant { color: var(--color-warning); }
  .mode-badge.system { color: var(--color-error); }
  .session-count { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .session-time { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .session-actions { flex-shrink: 0; display: flex; align-items: center; gap: 2px; }
  .open-btn { background: none; border: none; cursor: pointer; font-size: var(--fs-base); color: inherit; padding: 2px 4px; border-radius: var(--radius-sm); line-height: 1; display: inline-flex; align-items: center; justify-content: center; transition: opacity var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out); }
  .open-btn svg { display: block; }
  .open-btn:hover { opacity: 1 !important; background: var(--color-hover); }
  .copy-btn { background: none; border: none; cursor: pointer; font-size: var(--fs-base); color: inherit; padding: 2px 4px; border-radius: var(--radius-sm); line-height: 1; display: inline-flex; align-items: center; justify-content: center; transition: opacity var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out); }
  .copy-btn svg { display: block; }
  .copy-btn:hover { opacity: 1 !important; background: var(--color-hover); }
  .copy-btn.copied { color: var(--color-primary); opacity: 1 !important; visibility: visible; }
  .close-btn { background: none; border: none; cursor: pointer; color: inherit; padding: 2px 4px; border-radius: var(--radius-sm); line-height: 1; display: inline-flex; align-items: center; justify-content: center; transition: opacity var(--duration-fast) var(--ease-out); }
  .close-btn svg { display: block; }
  .close-btn:hover { opacity: 1 !important; background: var(--color-hover); }
  /* 仅支持 hover 的设备隐藏行操作按钮（hover/键盘聚焦时显示）；
     触屏（hover: none）始终可见，保证可发现性。见 .cursor/rules/ui-hover-reveal.mdc */
  @media (hover: hover) {
    .copy-btn,
    .open-btn,
    .close-btn {
      opacity: 0;
      visibility: hidden;
    }
    .session-item:hover .copy-btn,
    .session-item:focus-within .copy-btn,
    .session-item:hover .open-btn,
    .session-item:focus-within .open-btn,
    .session-item:hover .close-btn,
    .session-item:focus-within .close-btn {
      opacity: 0.6;
      visibility: visible;
    }
  }
</style>

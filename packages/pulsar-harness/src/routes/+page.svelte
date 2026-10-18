<script lang="ts">
  /**
   * 组合根：外壳（header + 四区布局）+ 依赖注入（ViewContext）。
   *
   * 布局区域：ActivityBar | sidebar | 中间列（main 分栏 + EditorTabs / 底部 panel）| info。
   * 底部 panel 归中间列，只占 main 宽度，左右栏整高不被截断。
   * 所有区域内容由 `layout/views.ts` 注册表驱动，本文件不承载任何业务逻辑；
   * 视图需要的数据/命令经 ViewContext 自取（见 ExampleView.svelte）。
   */
  import { onMount } from "svelte";
  import { api, c } from "$lib/api";
  import { t } from "$lib/i18n";
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import { layoutStore } from "$lib/layout/LayoutStore.svelte";
  import { activityItems, mainPanelMeta, mainViews } from "$lib/layout/views";
  import { setViewContext, type ViewUiState } from "$lib/layout/viewContext";
  import type { MainPane } from "$lib/layout/layoutTypes";
  import { BOUNDS, clamp } from "$lib/layout/layoutTypes";
  import ViewContainer from "$lib/layout/ViewContainer.svelte";
  import ViewHost from "$lib/layout/ViewHost.svelte";
  import EditorTabs from "$lib/layout/EditorTabs.svelte";
  import Splitter from "$lib/layout/Splitter.svelte";
  import ActivityBar from "$lib/layout/ActivityBar.svelte";
  import WindowEdgeResize from "$lib/layout/WindowEdgeResize.svelte";
  import ErrorBanner from "$lib/components/ErrorBanner.svelte";
  import ThemeSwitcher from "$lib/components/ThemeSwitcher.svelte";
  import LocaleSwitcher from "$lib/components/LocaleSwitcher.svelte";
  import { chrome } from "$lib/window/chrome";
  import { hotkeyService } from "$lib/hotkey/hotkeyService";
  import { applyThemeOnBoot } from "$lib/theme";
  import type { ViewMeta, ViewRegistration } from "$lib/layout/views";
  import IconPanelLeft from "~icons/lucide/panel-left";
  import IconPanelRight from "~icons/lucide/panel-right";
  import IconPanelBottom from "~icons/lucide/panel-bottom";
  import IconMinus from "~icons/lucide/minus";
  import IconSquare from "~icons/lucide/square";
  import IconSquareStack from "~icons/lucide/square-stack";
  import IconX from "~icons/lucide/x";

  const layout = layoutStore;

  // ── 视图命令与 UI 状态（组合根持有，经 ViewContext 下发）──
  let errorMsg = $state("");
  const ui = $state<ViewUiState>({});

  function showError(msg: string) {
    errorMsg = msg;
  }
  function dismissError() {
    errorMsg = "";
  }

  setViewContext({
    stores: { data: dataStore, layout: layoutStore },
    ui,
    commands: { showError, dismissError },
  });

  // ── main 区面板注册表（product type → component）──
  const mainRegistrations: Record<string, ViewRegistration> = Object.fromEntries(
    mainViews.map((v) => [v.id, v]),
  );

  function paneTabs(pane: MainPane): ViewMeta[] {
    return pane.panels.map((p) => ({
      id: p.id,
      label: mainPanelMeta[p.type].label,
      icon: mainPanelMeta[p.type].icon,
    }));
  }

  // ── 分栏宽度拖拽：按各栏实际像素宽把增量换算成 grow 权重 ──
  const paneEls: Record<string, HTMLElement> = {};

  function trackPane(node: HTMLElement, paneId: string) {
    paneEls[paneId] = node;
    return {
      destroy() {
        delete paneEls[paneId];
      },
    };
  }

  function resizePane(index: number, deltaPx: number) {
    const panes = layout.state.main.panes;
    const a = panes[index];
    const b = panes[index + 1];
    if (!a || !b) return;
    const totalWidth = panes.reduce(
      (sum, p) => sum + (paneEls[p.id]?.getBoundingClientRect().width ?? 0),
      0,
    );
    if (totalWidth <= 0) return;
    const totalGrow = a.grow + b.grow;
    const min = BOUNDS.paneGrow.min;
    const nextA = clamp(a.grow + (deltaPx / totalWidth) * totalGrow, min, totalGrow - min);
    if (!Number.isFinite(nextA) || nextA === a.grow) return;
    layout.setPaneGrow(a.id, nextA, false);
    layout.setPaneGrow(b.id, totalGrow - nextA, false);
  }

  /** ActivityBar 点击：切换「视图在侧栏的显隐」，并记录激活状态。 */
  function selectActivity(id: string) {
    const active = layout.state.activity.active === id;
    layout.setActivity(active ? null : id);
    if (active) {
      layout.hideView(id);
    } else if (layout.state.containers.sidebar.views.includes(id)) {
      layout.setContainerView("sidebar", id);
    } else {
      layout.showView(id, "sidebar");
    }
  }

  // ── 自绘标题栏：窗口控制（`tauri.conf.json` 为 `decorations: false`，标题栏由本 header 承载）──
  // 窗口 chrome 全部收在 `$lib/window/chrome`：模块内部判运行环境，非 Tauri（浏览器预览）
  // 自动降级为 no-op 桩；本文件只调 `chrome.*`、只读 `chrome.available`，不自行判断环境。
  // 与 `WindowEdgeResize.svelte`（无边框窗口边缘缩放热区）合起来才是完整的无边框 chrome。
  let isMaximized = $state(false);

  // ── 启动接线：主题、状态事件订阅、热键、诊断路径 ──
  let storagePath = $state<string | null>(null);

  onMount(() => {
    applyThemeOnBoot();
    dataStore.bootstrap();
    hotkeyService.initHotkeyService({ bindRoot: document.body });
    const offs = [
      hotkeyService.registerHotkey({ key: "b", ctrl: true }, () => layout.toggleSidebar()),
      hotkeyService.registerHotkey({ key: "j", ctrl: true }, () => layout.togglePanel()),
      hotkeyService.registerHotkey({ key: "i", ctrl: true, shift: true }, () => layout.toggleInfo()),
    ];
    void api
      .call(c.debugStoragePath, undefined)
      .then((p) => (storagePath = p))
      .catch(() => (storagePath = null));

    // 标题栏最大化态跟踪：随窗口 resize 同步，保证「最大化⇄还原」按钮图标与状态一致。
    const offResized = chrome.onResized(() => {
      void chrome.isMaximized().then((v) => (isMaximized = v));
    });
    void chrome.isMaximized().then((v) => (isMaximized = v));

    return () => {
      offs.forEach((off) => off());
      offResized();
      dataStore.dispose();
    };
  });
</script>

<div class="app">
  {#if chrome.available}
    <WindowEdgeResize />
  {/if}

  <ErrorBanner message={errorMsg} onDismiss={dismissError} />

  <!-- 标题栏承担窗口拖拽/双击最大化，属桌面 chrome 而非语义内容，跳过静态元素交互 a11y 检查 -->
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions -->
  <header
    class="app-header"
    onmousedown={chrome.startDrag}
    ondblclick={chrome.toggleMaximizeOnDoubleClick}
  >
    <span class="brand">pulsar-harness</span>
    <div class="header-actions">
      <button
        class="header-btn"
        class:active={layout.state.sidebar.visible}
        title={t("common.toggleSidebar")}
        aria-label={t("common.toggleSidebar")}
        onclick={() => layout.toggleSidebar()}
      >
        <IconPanelLeft />
      </button>
      <button
        class="header-btn"
        class:active={layout.state.info.visible}
        title={t("common.toggleInfo")}
        aria-label={t("common.toggleInfo")}
        onclick={() => layout.toggleInfo()}
      >
        <IconPanelRight />
      </button>
      <button
        class="header-btn"
        class:active={layout.state.panel.visible}
        title={t("common.togglePanel")}
        aria-label={t("common.togglePanel")}
        onclick={() => layout.togglePanel()}
      >
        <IconPanelBottom />
      </button>
      <button class="header-btn wide" onclick={() => layout.insertPanel("example")}>
        {t("common.newExamplePanel")}
      </button>
    </div>
    <div class="header-spacer"></div>
    {#if storagePath}
      <span class="storage-path" title={storagePath}>{storagePath}</span>
    {/if}
    <ThemeSwitcher />
    <LocaleSwitcher />
    {#if chrome.available}
      <span class="window-sep"></span>
      <div class="window-controls">
        <button
          class="win-btn"
          title={t("common.minimize")}
          aria-label={t("common.minimize")}
          onclick={() => chrome.minimize()}
        >
          <IconMinus />
        </button>
        <button
          class="win-btn"
          title={isMaximized ? t("common.restore") : t("common.maximize")}
          aria-label={isMaximized ? t("common.restore") : t("common.maximize")}
          onclick={() => chrome.toggleMaximize()}
        >
          {#if isMaximized}
            <IconSquareStack />
          {:else}
            <IconSquare />
          {/if}
        </button>
        <button
          class="win-btn win-btn-close"
          title={t("common.closeWindow")}
          aria-label={t("common.closeWindow")}
          onclick={() => chrome.close()}
        >
          <IconX />
        </button>
      </div>
    {/if}
  </header>

  <div class="app-body">
    <ActivityBar
      items={activityItems}
      activeId={layout.state.activity.active}
      onSelect={selectActivity}
    />

    {#if layout.state.sidebar.visible}
      <aside class="sidebar" style="width: {layout.state.sidebar.width}px">
        <ViewContainer containerId="sidebar" />
      </aside>
      <Splitter
        orientation="vertical"
        onResize={(d) => layout.setSidebarWidth(layout.state.sidebar.width + d, false)}
        onResizeEnd={() => layout.persistNow()}
      />
    {/if}

    <!-- 中间列：main（分栏 + EditorTabs）+ 底部 panel（panel 只占 main 宽度，左右栏整高不被截断） -->
    <div class="center-column">
      <main class="main">
        {#if layout.state.main.panes.length === 0}
          <p class="main-empty">{t("common.newExamplePanel")}</p>
        {:else}
          {#each layout.state.main.panes as pane, i (pane.id)}
            <section class="pane" style="flex-grow: {pane.grow}" use:trackPane={pane.id}>
              <EditorTabs
                tabs={paneTabs(pane)}
                activeId={pane.activePanelId}
                paneId={pane.id}
                onSelect={(id) => layout.setActivePanel(id)}
                onClose={(id) => layout.closePanel(id)}
                onDrop={(panelId, targetPaneId, targetIndex) =>
                  layout.movePanel(panelId, targetPaneId, targetIndex)}
                onDropToNewPane={(panelId) => layout.movePanelToNewPane(panelId)}
              />
              <div class="pane-body">
                {#each pane.panels as panel (panel.id)}
                  <div class="panel-slot" class:inactive={panel.id !== pane.activePanelId}>
                    <ViewHost registration={mainRegistrations[panel.type]} {panel} />
                  </div>
                {/each}
              </div>
            </section>
            {#if i < layout.state.main.panes.length - 1}
              <Splitter
                orientation="vertical"
                onResize={(d) => resizePane(i, d)}
                onResizeEnd={() => layout.persistNow()}
              />
            {/if}
          {/each}
        {/if}
      </main>

      {#if layout.state.panel.visible}
        <Splitter
          orientation="horizontal"
          onResize={(d) => layout.setPanelHeight(layout.state.panel.height - d, false)}
          onResizeEnd={() => layout.persistNow()}
        />
        <section class="panel-region" style="height: {layout.state.panel.height}px">
          <ViewContainer containerId="panel" />
        </section>
      {/if}
    </div>

    {#if layout.state.info.visible}
      <Splitter
        orientation="vertical"
        onResize={(d) => layout.setInfoWidth(layout.state.info.width - d, false)}
        onResizeEnd={() => layout.persistNow()}
      />
      <aside class="info" style="width: {layout.state.info.width}px">
        <ViewContainer containerId="info" />
      </aside>
    {/if}
  </div>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    background: var(--color-bg);
    color: var(--color-text);
    font-family: var(--font-body);
  }

  .app-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-shrink: 0;
    height: 36px;
    padding: 0 var(--space-2);
    background: var(--color-surface);
    border-bottom: var(--border-width) solid var(--color-border);
    /* 自绘标题栏：整行禁选文本，避免选中内容干扰窗口拖拽 */
    user-select: none;
    -webkit-user-select: none;
  }

  .brand {
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.02em;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .header-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    height: 24px;
    padding: 0 var(--space-1);
    border: var(--border-width) solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    font-size: var(--fs-sm);
    line-height: 1;
    cursor: pointer;
  }
  .header-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }
  .header-btn.active {
    border-color: var(--color-primary);
    color: var(--color-primary);
  }
  .header-btn.wide {
    font-size: var(--fs-xs);
    padding: 0 var(--space-2);
  }

  /* 图标组件（lucide）尺寸：标题栏折叠按钮与内容同高 */
  .header-btn :global(svg) {
    display: block;
    width: 14px;
    height: 14px;
  }

  .header-spacer {
    flex: 1;
  }

  .storage-path {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }

  /* ── 窗口控制按钮（自绘标题栏）── */
  .window-sep {
    width: 1px;
    height: 20px;
    background: var(--color-border);
    margin: 0 var(--space-1);
  }

  .window-controls {
    display: flex;
    align-items: center;
  }

  .win-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 24px;
    padding: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    line-height: 1;
    cursor: pointer;
  }
  .win-btn :global(svg) {
    display: block;
    width: 14px;
    height: 14px;
  }
  .win-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }
  .win-btn-close:hover {
    background: var(--color-error);
    color: #fff;
  }

  .app-body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .sidebar {
    flex-shrink: 0;
    min-width: 0;
    display: flex;
    border-right: var(--border-width) solid var(--color-border);
  }

  /* 中间列：main 与底部 panel 纵向排列；panel 宽度随本列，左右栏保持整高 */
  .center-column {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .main {
    display: flex;
    flex: 1;
    min-width: 0;
    min-height: 0;
    background: var(--color-bg);
  }

  .main-empty {
    margin: auto;
    font-size: var(--fs-sm);
    color: var(--color-text-muted);
  }

  .pane {
    display: flex;
    flex-direction: column;
    flex-basis: 0;
    min-width: 0;
    min-height: 0;
  }

  .pane-body {
    position: relative;
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .panel-slot {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
  }

  .panel-slot.inactive {
    display: none;
  }

  .info {
    flex-shrink: 0;
    min-width: 0;
    display: flex;
    border-left: var(--border-width) solid var(--color-border);
  }

  .panel-region {
    flex-shrink: 0;
    display: flex;
    border-top: var(--border-width) solid var(--color-border);
    background: var(--color-surface);
  }
</style>

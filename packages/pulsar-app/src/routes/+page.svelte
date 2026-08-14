<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import SessionList from "$lib/components/SessionList.svelte";
  import ProvidersModelsPanel from "$lib/components/ProvidersModelsPanel.svelte";
  import TopicPanel from "$lib/components/TopicPanel.svelte";
  import SessionCreateModal from "$lib/components/SessionCreateModal.svelte";
  import ErrorBanner from "$lib/components/ErrorBanner.svelte";
  import ActivityBar from "$lib/layout/ActivityBar.svelte";
  import Splitter from "$lib/layout/Splitter.svelte";
  import EditorTabs from "$lib/layout/EditorTabs.svelte";
  import ViewHost from "$lib/layout/ViewHost.svelte";
  import ViewContainer from "$lib/layout/ViewContainer.svelte";
  import { layoutStore } from "$lib/layout/LayoutStore.svelte";
  import { activityItems, mainViews, mainPanelMeta } from "$lib/layout/views";
  import { setViewContext, type ViewContext } from "$lib/layout/viewContext";
  import { t } from "$lib/i18n";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import { hotkeyService } from "$lib/hotkey/hotkeyService";
  import { dataStore } from "$lib/stores/dataStore.svelte";

  // ── 统一数据（dataStore 驱动：bootstrap + 事件订阅刷新）──
  let conversations = $derived(dataStore.state.conversations);
  let runtimeStatus = $derived(dataStore.state.runtimeStatus);
  let ready = $derived(dataStore.state.ready);

  // ── Active selection（会话选择由 dataStore 管理；provider/model 持久化到 localStorage）──
  let activeConversationId = $derived(dataStore.state.activeConversationId ?? "");

  // 读取 localStorage 选择项并一次性迁移旧键（agent-app:* → pulsar:*）
  function persistedSelection(key: string, legacyKey: string): string {
    const value = localStorage.getItem(key);
    if (value === null) {
      const legacy = localStorage.getItem(legacyKey);
      if (legacy !== null) {
        localStorage.setItem(key, legacy);
        localStorage.removeItem(legacyKey);
        return legacy;
      }
    }
    return value ?? "";
  }

  // ── ViewContext：视图共享的会话级 UI 状态（$state 保证响应式传播）──
  // 运行状态由 dataStore.runningSessions 权威驱动（后端多会话并行）；
  // sendingIds 仅做本会话发送请求的瞬时防连点，互不阻塞其他会话。
  let ui = $state({
    activeProviderId: persistedSelection("pulsar:providerId", "agent-app:providerId"),
    activeModelId: persistedSelection("pulsar:modelId", "agent-app:modelId"),
    sendingIds: new Set<string>(),
  });

  // ── UI state ──
  let error = $state("");
  let showCreateModal = $state(false);
  let drawerSidebar = $state(false);
  let drawerInfo = $state(false);
  // 移动端 drawer-info：原 Info 组合面板拆分后的聚合视图，drawer 内以本地 tab 切换承载
  let drawerInfoTab = $state("providers-models");
  let infoDrawerTabs = $derived([
    { id: "providers-models", label: t("views.providersModels") },
    { id: "topics", label: t("topicPanel.topics") },
  ]);

  // ── Layout (store-driven) ──
  let mainRef = $state<HTMLElement | null>(null);
  // main 区面板：用户交互插入/关闭，默认空；同一类型全局唯一
  let mainPanes = $derived(layoutStore.state.main.panes);
  let activePaneId = $derived(layoutStore.state.main.activePaneId);
  // 当前激活分栏（用于 ActivityBar 高亮判定）
  let activePane = $derived(mainPanes.find((p) => p.id === activePaneId));
  // ActivityBar 高亮：chat 面板激活时点亮对话入口，否则跟随侧栏活动
  let activityBarActive = $derived(
    activePane?.panels.some((x) => x.id === activePane.activePanelId && x.type === "chat")
      ? "chat"
      : layoutStore.state.activity.active
  );
  let sidebarStyle = $derived(
    layoutStore.state.sidebar.visible
      ? `width:${layoutStore.state.sidebar.width}px`
      : "width:0"
  );
  let infoStyle = $derived(
    layoutStore.state.info.visible
      ? `width:${layoutStore.state.info.width}px`
      : "width:0"
  );
  let panelStyle = $derived(
    layoutStore.state.panel.visible
      ? `height:${layoutStore.state.panel.height}px`
      : "height:0"
  );

  // ── Derived ──
  let activeConversation = $derived(
    conversations.find((c) => c.id === activeConversationId)
  );
  let activeMode = $derived(activeConversation?.mode ?? "chat");
  let hasModel = $derived(!!ui.activeProviderId && !!ui.activeModelId);

  // ── Bootstrap：统一拉取 + 订阅后端状态事件 ──
  onMount(async () => {
    await dataStore.bootstrap();
    await dataStore.subscribe();
    setupHotkeys();
    void setWindowIcon();
  });

  /** 运行时设置窗口图标（Linux 桌面导航栏/任务栏显示）。
   * 打包安装后由 .desktop 图标接管；dev 模式与未打包场景依赖此调用生效。 */
  async function setWindowIcon() {
    try {
      const res = await fetch("/favicon.png");
      if (!res.ok) return;
      const bytes = new Uint8Array(await res.arrayBuffer());
      await getCurrentWindow().setIcon(bytes);
    } catch {
      // 非 Tauri 环境或失败时静默，不影响应用启动
    }
  }

  async function handleSend(text: string) {
    // 发送目标在发起时固定：期间切换会话不得改变校验/清理目标，
    // 否则 sendingIds 会残留错误会话（该会话永久显示"思考中"且无法发送）。
    const conversationId = dataStore.state.activeConversationId;
    if (!conversationId) {
      error = "No active session. Create a new session first.";
      return;
    }
    if (!hasModel) {
      error = "Select a provider and model before sending.";
      return;
    }
    // sendingIds 仅为发送按钮防抖锁：运行状态由后端 runningSessions 权威驱动，
    // 此处拦截后端 register 事件回来前的同会话连点重复发送。
    if (ui.sendingIds.has(conversationId)) return;
    error = "";
    ui.sendingIds = new Set(ui.sendingIds).add(conversationId);
    try {
      await dataStore.sendMessage(text, ui.activeProviderId, ui.activeModelId);
    } catch (e) {
      error = `Send failed: ${formatInvokeError(e)}`;
    } finally {
      const next = new Set(ui.sendingIds);
      next.delete(conversationId);
      ui.sendingIds = next;
    }
  }

  async function handleCreateSession(mode: string) {
    showCreateModal = false;
    try {
      await dataStore.createConversation(mode);
    } catch (e) {
      error = `Failed to create session: ${formatInvokeError(e)}`;
    }
  }

  async function handleCloseSession(sessionId: string) {
    try {
      // dataStore 内部处理 active 回退与列表/消息刷新。
      await dataStore.closeSession(sessionId);
    } catch (e) {
      error = `Failed to close session: ${formatInvokeError(e)}`;
    }
  }

  function handleSelectConversation(id: string) {
    void dataStore.selectConversation(id);
    // 保证会话面板存在（同一类型全局唯一，已存在则激活）
    layoutStore.insertPanel("chat");
    drawerSidebar = false;
  }

  function handleModelChange(providerId: string, modelId: string) {
    ui.activeProviderId = providerId;
    ui.activeModelId = modelId;
    localStorage.setItem("pulsar:providerId", providerId);
    localStorage.setItem("pulsar:modelId", modelId);
  }

  // ── ViewContext：容器与内容解耦的边界（容器只消费注册表，视图组件自取 context）──
  function viewForType(type: string) {
    return mainViews.find((v) => v.id === type)!;
  }

  // 工具配置编辑面板：插入/激活 tool-editor 面板（默认第 0 栏，同一类型全局唯一）
  function openToolEditor() {
    layoutStore.insertPanel("tool-editor");
  }
  function closeToolEditor() {
    const panel = mainPanes.flatMap((p) => p.panels).find((x) => x.type === "tool-editor");
    if (panel) layoutStore.closePanel(panel.id);
  }
  function closeProviderManager() {
    const panel = mainPanes.flatMap((p) => p.panels).find((x) => x.type === "provider-manager");
    if (panel) layoutStore.closePanel(panel.id);
  }

  const viewCtx: ViewContext = {
    stores: { data: dataStore, layout: layoutStore },
    ui,
    commands: {
      sendMessage: handleSend,
      selectConversation: handleSelectConversation,
      createSession: handleCreateSession,
      closeSession: handleCloseSession,
      changeModel: handleModelChange,
      openCreateModal: () => {
        showCreateModal = true;
        drawerSidebar = false;
        drawerInfo = false;
      },
      showError: (msg) => (error = msg),
      dismissError: () => (error = ""),
      openToolEditor,
      closeToolEditor,
      closeProviderManager,
    },
  };
  setViewContext(viewCtx);

  // ── Activity Bar / 布局操作 ──

  /** 神经元面板：只注册一个（默认创建到第 2 栏），再次点击关闭。 */
  function toggleNeuronPanel() {
    const panel = mainPanes.flatMap((p) => p.panels).find((x) => x.type === "neurons");
    if (panel) layoutStore.closePanel(panel.id);
    else layoutStore.insertPanel("neurons", 2);
  }

  function handleActivitySelect(id: string) {
    const active = layoutStore.state.activity.active;
    if (id === "sessions") {
      if (active === id && layoutStore.state.sidebar.visible) { layoutStore.toggleSidebar(); return; }
      layoutStore.setActivity("sessions");
      if (!layoutStore.state.sidebar.visible) layoutStore.toggleSidebar();
      return;
    }
    if (id === "info") {
      if (active === id && layoutStore.state.info.visible) { layoutStore.toggleInfo(); return; }
      layoutStore.setActivity("info");
      if (!layoutStore.state.info.visible) layoutStore.toggleInfo();
      return;
    }
    if (id === "chat") { layoutStore.insertPanel("chat"); return; }
    if (id === "neurons") { toggleNeuronPanel(); return; }
  }

  /** 分栏内 tab ✕ 关闭：关闭对应面板（分栏空则自动收缩）。 */
  function handleTabClose(panelId: string) {
    layoutStore.closePanel(panelId);
  }

  /** 分栏内 tab 列表：由该分栏的面板动态生成。 */
  function paneTabs(pane: (typeof mainPanes)[number]) {
    return pane.panels.map((p) => ({
      id: p.id,
      label: mainPanelMeta[p.type].label,
      icon: mainPanelMeta[p.type].icon,
    }));
  }

  /** 拖拽第 i 个分栏分隔条：调整相邻两栏 grow 权重。 */
  function handlePaneResize(i: number, delta: number) {
    const containerW = mainRef?.clientWidth ?? 800;
    const left = mainPanes[i];
    const right = mainPanes[i + 1];
    if (!left || !right) return;
    const total = left.grow + right.grow;
    const leftPx = containerW * (left.grow / total);
    const ratio = Math.max(-0.9, Math.min(0.9, delta / Math.max(1, leftPx)));
    const newLeftGrow = left.grow * (1 + ratio);
    const newRightGrow = Math.max(0.2, total - newLeftGrow);
    layoutStore.setPaneGrow(left.id, newLeftGrow, false);
    layoutStore.setPaneGrow(right.id, newRightGrow, false);
  }

  /** 拖拽经过分栏（内容区/空白区）时允许放置，作为跨分栏移动的落点。 */
  function handlePaneDragOver(e: DragEvent) {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  }

  /** 拖拽到分栏内容区：追加面板到该分栏（tab 栏上的放置由 EditorTabs 拦截处理）。 */
  function handlePaneDrop(e: DragEvent, paneId: string) {
    e.preventDefault();
    const panelId = e.dataTransfer?.getData("text/plain");
    if (!panelId) return;
    const pane = mainPanes.find((p) => p.id === paneId);
    if (!pane) return;
    layoutStore.movePanel(panelId, paneId, pane.panels.length);
  }

  // ── 快捷键服务（单例）──
  // 初始化时一次性约定绑定的 DOM 根 + 忽略规则；运行时仅注册 combo + 回调。
  // 未命中任何 combo 时服务不 preventDefault，系统/浏览器快捷键（Ctrl+T/W/R 等）恢复正常。
  function setupHotkeys() {
    hotkeyService.initHotkeyService({
      bindRoot: document.body, // 覆盖全局含 drawer
      ignoreInput: true, // 可输入区内按键默认放行
    });

    // 原硬代码快捷键迁移为声明式注册（绑定到全局根）
    hotkeyService.registerHotkey({ key: "j", ctrl: true, shift: true }, () =>
      layoutStore.togglePanel()
    );
    hotkeyService.registerHotkey({ key: "j", ctrl: true }, () => {
      showCreateModal = true;
    });
    hotkeyService.registerHotkey({ key: "b", ctrl: true }, () =>
      layoutStore.toggleSidebar()
    );
    hotkeyService.registerHotkey({ key: "i", ctrl: true }, () =>
      layoutStore.toggleInfo()
    );
    hotkeyService.registerHotkey({ key: "\\", ctrl: true }, () => {
      toggleNeuronPanel();
    });
  }

  // Esc 单独处理（保持原 drawer 关闭行为），不进服务
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      drawerSidebar = false;
      drawerInfo = false;
    }
  }

  function closeDrawers() {
    drawerSidebar = false;
    drawerInfo = false;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="app-layout">
  <nav class="activity-area">
    <ActivityBar
      items={activityItems}
      activeId={activityBarActive}
      onSelect={handleActivitySelect}
    />
  </nav>

  <header class="status-area">
    <StatusBar
      appName={runtimeStatus?.app_name ?? "星脉"}
      sessionId={activeConversationId}
      mode={activeMode}
      sidebarVisible={layoutStore.state.sidebar.visible}
      infoVisible={layoutStore.state.info.visible}
      panelVisible={layoutStore.state.panel.visible}
      onToggleSidebar={() => {
        if (window.innerWidth <= 800) drawerSidebar = !drawerSidebar;
        else layoutStore.toggleSidebar();
      }}
      onToggleInfo={() => {
        if (window.innerWidth <= 800) drawerInfo = !drawerInfo;
        else layoutStore.toggleInfo();
      }}
      onTogglePanel={() => layoutStore.togglePanel()}
    />
  </header>

  <!-- Desktop sidebar -->
  <div class="main-area">
    <aside class="sidebar-area desktop-only" style={sidebarStyle}>
      <ViewContainer containerId="sidebar" />
    </aside>

    <Splitter
      orientation="vertical"
      extraClass="desktop-only"
      onResize={(delta) => layoutStore.setSidebarWidth(layoutStore.state.sidebar.width + delta, false)}
      onResizeEnd={() => layoutStore.persistNow()}
    />

    <!-- Center column: editor + bottom panel -->
    <div class="center-column">
      <main class="chat-area" bind:this={mainRef}>
        <div class="chat-content">
          {#if mainPanes.length === 0}
            <div class="main-empty">
              <p>{t("common.mainEmpty")}</p>
            </div>
          {:else}
            <div class="main-panes">
              {#each mainPanes as pane, i (pane.id)}
                {#if i > 0}
                  <Splitter
                    orientation="vertical"
                    onResize={(delta) => handlePaneResize(i - 1, delta)}
                    onResizeEnd={() => layoutStore.persistNow()}
                  />
                {/if}
                <div
                  class="main-pane"
                  role="group"
                  class:active={pane.id === activePaneId}
                  style="flex-grow: {pane.grow};"
                  ondragover={handlePaneDragOver}
                  ondrop={(e) => handlePaneDrop(e, pane.id)}
                >
                  <!-- 每个分栏一个独立 tab 列表（分栏内面板可切换/拖拽重排/跨分栏移动） -->
                  <EditorTabs
                    tabs={paneTabs(pane)}
                    activeId={pane.activePanelId}
                    paneId={pane.id}
                    onSelect={(panelId) => layoutStore.setActivePanel(panelId)}
                    onClose={handleTabClose}
                    onDrop={(panelId, targetPaneId, targetIndex) =>
                      layoutStore.movePanel(panelId, targetPaneId, targetIndex)}
                    onDropToNewPane={(panelId) => layoutStore.movePanelToNewPane(panelId)}
                  />
                  <div class="pane-content">
                    {#each pane.panels as panel (panel.id)}
                      <div class="pane-view" class:visible={panel.id === pane.activePanelId}>
                        <ViewHost registration={viewForType(panel.type)} />
                      </div>
                    {/each}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </main>

      <!-- Bottom panel: only under the center main area -->
      <section class="panel-area desktop-only" style={panelStyle}>
        <Splitter
          orientation="horizontal"
          onResize={(delta) => layoutStore.setPanelHeight(layoutStore.state.panel.height - delta, false)}
          onResizeEnd={() => layoutStore.persistNow()}
        />
        <ViewContainer containerId="panel" />
      </section>
    </div>

    <Splitter
      orientation="vertical"
      extraClass="desktop-only"
      onResize={(delta) => layoutStore.setInfoWidth(layoutStore.state.info.width - delta, false)}
      onResizeEnd={() => layoutStore.persistNow()}
    />

    <!-- Desktop info panel -->
    <aside class="info-area desktop-only" style={infoStyle}>
      <ViewContainer containerId="info" />
    </aside>
  </div>

  <div class="error-area">
    <ErrorBanner
      message={error || dataStore.state.error}
      onDismiss={() => { error = ""; dataStore.state.error = ""; }}
    />
  </div>
</div>

<!-- Mobile drawer overlays -->
{#if drawerSidebar}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="drawer-backdrop" role="presentation" onclick={closeDrawers}></div>
  <aside class="drawer drawer-left">
    <div class="drawer-header">
      <h2>{t("drawer.sessions")}</h2>
      <button class="drawer-close" onclick={closeDrawers}>×</button>
    </div>
    <SessionList />
  </aside>
{/if}

{#if drawerInfo}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="drawer-backdrop" role="presentation" onclick={closeDrawers}></div>
  <aside class="drawer drawer-right">
    <div class="drawer-header">
      <h2>{t("drawer.info")}</h2>
      <button class="drawer-close" onclick={closeDrawers}>×</button>
    </div>
    <div class="drawer-tabs">
      {#each infoDrawerTabs as tab}
        <button
          class="drawer-tab"
          class:active={drawerInfoTab === tab.id}
          onclick={() => (drawerInfoTab = tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </div>
    <div class="drawer-body">
      {#if drawerInfoTab === "providers-models"}
        <ProvidersModelsPanel />
      {:else}
        <TopicPanel />
      {/if}
    </div>
  </aside>
{/if}

<SessionCreateModal
  open={showCreateModal}
  onCreate={handleCreateSession}
  onClose={() => (showCreateModal = false)}
/>

{#if !ready}
  <div class="loading-overlay">
    <p>Loading...</p>
  </div>
{/if}

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica,
      Arial, sans-serif;
    font-size: 14px;
    line-height: 1.5;
    background: var(--color-bg);
    color: var(--color-text);
    overflow: hidden;
  }

  :global(*, *::before, *::after) {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
  }

  :global(h1, h2, h3, h4, h5, h6, p, ul, ol, pre) {
    margin: 0;
  }

  .app-layout {
    display: grid;
    height: 100%;
    grid-template-columns: auto 1fr;
    grid-template-rows: auto 1fr auto;
    grid-template-areas:
      "status status"
      "activity main"
      "error error";
    overflow: hidden;
  }

  .activity-area { grid-area: activity; display: flex; min-height: 0; }
  .status-area { grid-area: status; }

  .main-area {
    grid-area: main;
    display: flex;
    align-items: stretch;
    min-width: 0;
    min-height: 0;
  }

  /* 中间列：编辑区 + 底部面板（左右栏保持整高，不被底栏截断） */
  .center-column {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .sidebar-area {
    flex: none;
    width: 260px;
    overflow: hidden;
    background: var(--color-surface);
    border-right: var(--border-width) solid var(--color-border);
  }

  .chat-area {
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .chat-content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* main 区空态：默认无面板，提示从入口插入 */
  .main-empty {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    font-size: var(--fs-sm);
  }

  /* main 区分栏：flex 并排，分栏间由 Splitter 分隔；激活栏边框高亮 */
  .main-panes {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
  }
  .main-pane {
    flex: 1 1 0;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-right: var(--border-width) solid transparent;
  }
  .main-pane.active {
    border-right-color: var(--color-border);
  }
  /* 分栏内容区：tab 栏下方占据剩余高度。
     用单行 grid（1fr 轨道）而非 flex 撑高：grid 轨道尺寸是确定值，
     panel 根节点可用 height:100% 取全高（flex 撑高在 WebKitGTK 下百分比解析不可靠）。 */
  .pane-content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: 1fr;
  }
  /* 分栏内面板：仅激活面板可见，其余保持挂载（隐藏）以保留状态。
     所有面板共用第 1 行轨道（隐藏项 display:none 不生成轨道，不参与布局）。 */
  .pane-view {
    grid-row: 1;
    min-width: 0;
    min-height: 0;
    display: none;
  }
  /* 列方向 flex：子元素（视图组件）在交叉轴（宽度）上 stretch 继承外层宽度 */
  .pane-view.visible {
    display: flex;
    flex-direction: column;
  }

  .info-area {
    flex: none;
    width: 280px;
    overflow: hidden;
    background: var(--color-surface);
    border-left: var(--border-width) solid var(--color-border);
  }

  .panel-area {
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    background: var(--color-surface);
    border-top: var(--border-width) solid var(--color-border);
  }

  .error-area { grid-area: error; }

  .loading-overlay {
    position: fixed; inset: 0;
    display: flex; align-items: center; justify-content: center;
    background: var(--color-bg); z-index: 200;
  }
  .loading-overlay p { font-size: 16px; color: var(--color-text-muted); }

  /* ── Drawers ── */
  .drawer-backdrop {
    position: fixed; inset: 0; z-index: 50;
    background: rgba(0, 0, 0, 0.3);
  }

  .drawer {
    position: fixed; top: 0; bottom: 0; z-index: 60;
    width: 300px; max-width: 85vw;
    background: var(--color-surface);
    border-right: var(--border-width) solid var(--color-border);
    display: flex; flex-direction: column;
    animation: drawer-slidein var(--duration-normal) var(--ease-out);
  }

  .drawer-left { left: 0; }
  .drawer-right { right: 0; border-right: none; border-left: var(--border-width) solid var(--color-border); }

  @keyframes drawer-slidein {
    from { transform: translateX(-20px); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
  }

  .drawer-right {
    animation-name: drawer-slidein-right;
  }

  @keyframes drawer-slidein-right {
    from { transform: translateX(20px); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
  }

  .drawer-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: var(--space-3) var(--space-4);
    border-bottom: var(--border-width) solid var(--color-border);
  }

  .drawer-header h2 {
    margin: 0; font-size: var(--fs-base); font-weight: 600;
  }

  .drawer-close {
    background: none; border: none; font-size: 22px; cursor: pointer;
    color: var(--color-text); padding: 0 4px; line-height: 1;
  }

  .drawer :global(.sidebar) {
    width: 100% !important;
    border-right: none;
  }

  .drawer-tabs {
    display: flex;
    border-bottom: var(--border-width) solid var(--color-border);
    flex-shrink: 0;
  }
  .drawer-tab {
    flex: 1;
    padding: var(--space-2);
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: var(--fs-sm);
    font-weight: 500;
    color: var(--color-text-muted);
    border-bottom: 2px solid transparent;
  }
  .drawer-tab.active { color: var(--color-primary); border-bottom-color: var(--color-primary); }

  .drawer-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .drawer-body :global(> *) { flex: 1; min-height: 0; }

  /* ── Responsive: <800px hide desktop panels, show drawers ── */
  @media (max-width: 800px) {
    .desktop-only { display: none; }

    .app-layout {
      grid-template-rows: auto 1fr auto;
      grid-template-areas:
        "status status"
        "activity main"
        "error error";
    }

    .main-area { min-width: 0; }
  }

  @media (min-width: 801px) {
    /* On desktop, only show sidebar inline when not in drawer mode */
    .drawer-backdrop,
    .drawer { display: none; }
  }
</style>

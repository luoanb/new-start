<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import SessionList from "$lib/components/SessionList.svelte";
  import ProvidersPanel from "$lib/components/ProvidersPanel.svelte";
  import ModelsPanel from "$lib/components/ModelsPanel.svelte";
  import TopicPanel from "$lib/components/TopicPanel.svelte";
  import SessionCreateModal from "$lib/components/SessionCreateModal.svelte";
  import ErrorBanner from "$lib/components/ErrorBanner.svelte";
  import ActivityBar from "$lib/layout/ActivityBar.svelte";
  import Splitter from "$lib/layout/Splitter.svelte";
  import EditorTabs from "$lib/layout/EditorTabs.svelte";
  import ViewHost from "$lib/layout/ViewHost.svelte";
  import ViewContainer from "$lib/layout/ViewContainer.svelte";
  import { layoutStore } from "$lib/layout/LayoutStore.svelte";
  import { activityItems, mainViews, mainTabs } from "$lib/layout/views";
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

  // ── ViewContext：视图共享的会话级 UI 状态（$state 保证响应式传播）──
  // 运行状态由 dataStore.runningSessions 权威驱动（后端多会话并行）；
  // sendingIds 仅做本会话发送请求的瞬时防连点，互不阻塞其他会话。
  let ui = $state({
    activeProviderId: localStorage.getItem("agent-app:providerId") ?? "",
    activeModelId: localStorage.getItem("agent-app:modelId") ?? "",
    sendingIds: new Set<string>(),
  });

  // ── UI state ──
  let error = $state("");
  let showCreateModal = $state(false);
  let drawerSidebar = $state(false);
  let drawerInfo = $state(false);
  // 移动端 drawer-info：原 Info 组合面板拆分为三个独立视图，drawer 内以本地 tab 切换承载
  let drawerInfoTab = $state("providers");
  let infoDrawerTabs = $derived([
    { id: "providers", label: t("sidePanel.providers") },
    { id: "models", label: t("sidePanel.models") },
    { id: "topics", label: t("topicPanel.topics") },
  ]);

  // ── Layout (store-driven) ──
  let mainRef = $state<HTMLElement | null>(null);
  // split 状态的唯一真源：main.splits 非空即处于 chat|neurons 分栏
  let isNeuronSplit = $derived(
    layoutStore.state.main.splits.length > 0
  );
  let splitRatio = $derived(
    layoutStore.state.main.splits[0]?.ratio ?? 0.5
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
    if (!activeConversationId) {
      error = "No active session. Create a new session first.";
      return;
    }
    if (!hasModel) {
      error = "Select a provider and model before sending.";
      return;
    }
    // 仅拦截当前会话自身的并发发送；其他会话并行不受影响
    if (ui.sendingIds.has(activeConversationId)) return;
    error = "";
    ui.sendingIds = new Set(ui.sendingIds).add(activeConversationId);
    try {
      await dataStore.sendMessage(text, ui.activeProviderId, ui.activeModelId);
    } catch (e) {
      error = `Send failed: ${formatInvokeError(e)}`;
    } finally {
      const next = new Set(ui.sendingIds);
      next.delete(activeConversationId);
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
    drawerSidebar = false;
  }

  function handleModelChange(providerId: string, modelId: string) {
    ui.activeProviderId = providerId;
    ui.activeModelId = modelId;
    localStorage.setItem("agent-app:providerId", providerId);
    localStorage.setItem("agent-app:modelId", modelId);
  }

  // ── ViewContext：容器与内容解耦的边界（容器只消费注册表，视图组件自取 context）──
  const chatView = mainViews.find((v) => v.id === "chat")!;
  const neuronsView = mainViews.find((v) => v.id === "neurons")!;

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
    },
  };
  setViewContext(viewCtx);

  // ── Activity Bar / 布局操作 ──

  /** 打开/关闭 neuron split（chat | neurons 并排）。状态真源 = main.splits */
  function toggleNeuronSplit() {
    if (layoutStore.state.main.splits.length > 0) {
      layoutStore.setActivity("chat");
      layoutStore.setMainSplits([]);
    } else {
      layoutStore.setActivity("neurons");
      layoutStore.setMainSplits([{ id: "chat", orientation: "vertical", ratio: 0.5 }]);
    }
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
    if (id === "neurons") { toggleNeuronSplit(); return; }
    if (id === "chat") { layoutStore.setActivity("chat"); return; }
  }

  /** 主区 tab ✕ 关闭：关闭对应面板，恢复单视图（保留另一个） */
  function handleTabClose(id: string) {
    layoutStore.setMainSplits([]);
    layoutStore.setActivity(id === "chat" ? "neurons" : "chat");
  }

  function handleSplitResize(delta: number) {
    const containerW = mainRef?.clientWidth ?? 800;
    layoutStore.updateMainSplitRatio(splitRatio + delta / containerW, false);
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
      toggleNeuronSplit();
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
      activeId={layoutStore.state.activity.active}
      onSelect={handleActivitySelect}
    />
  </nav>

  <header class="status-area">
    <StatusBar
      appName={runtimeStatus?.app_name ?? "Agent App"}
      sessionId={activeConversationId}
      mode={activeMode}
      neuronActive={isNeuronSplit}
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
      onToggleNeuron={() => handleActivitySelect("neurons")}
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
        <EditorTabs
          tabs={mainTabs}
          activeId={layoutStore.state.activity.active}
          split={isNeuronSplit}
          onSelect={(id) => layoutStore.setActivity(id)}
          onClose={handleTabClose}
        />
        <div class="chat-content">
          {#if isNeuronSplit}
            <div class="main-split" style="--split-ratio: {splitRatio}">
              <ViewHost registration={chatView} />
              <Splitter
                orientation="vertical"
                onResize={handleSplitResize}
                onResizeEnd={() => layoutStore.persistNow()}
              />
              <ViewHost registration={neuronsView} />
            </div>
          {:else if layoutStore.state.activity.active === "neurons"}
            <ViewHost registration={neuronsView} />
          {:else}
            <ViewHost registration={chatView} />
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
      {#if drawerInfoTab === "providers"}
        <ProvidersPanel />
      {:else if drawerInfoTab === "models"}
        <ModelsPanel />
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

  .main-split {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-columns:
      minmax(0, calc(var(--split-ratio) * (100% - 4px))) auto
      minmax(0, calc((1 - var(--split-ratio)) * (100% - 4px)));
  }
  .main-split > :global(*) { min-width: 0; min-height: 0; }

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

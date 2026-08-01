<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type {
    ProviderInfo,
    ModelInfo,
    SkillInfo,
    Conversation,
    Message,
    ChatResponse,
    RuntimeStatus,
    Topic,
    PollerStatus,
  } from "$lib/types";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import SessionList from "$lib/components/SessionList.svelte";
  import ChatArea from "$lib/components/ChatArea.svelte";
  import SidePanel from "$lib/components/SidePanel.svelte";
  import SessionCreateModal from "$lib/components/SessionCreateModal.svelte";
  import ErrorBanner from "$lib/components/ErrorBanner.svelte";
  import NeuronManager from "$lib/components/NeuronManager.svelte";
  import PollerPanel from "$lib/components/PollerPanel.svelte";
  import LogPanel from "$lib/components/LogPanel.svelte";
  import ActivityBar from "$lib/layout/ActivityBar.svelte";
  import Splitter from "$lib/layout/Splitter.svelte";
  import DockPane from "$lib/layout/DockPane.svelte";
  import EditorTabs from "$lib/layout/EditorTabs.svelte";
  import { layoutStore } from "$lib/layout/LayoutStore.svelte";
  import { activityItems, panelViews, mainTabs } from "$lib/layout/views";
  import { t } from "$lib/i18n";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";

  // ── Bootstrap state (loaded once) ──
  let providers: ProviderInfo[] = $state([]);
  let models: ModelInfo[] = $state([]);
  let skills: SkillInfo[] = $state([]);
  let conversations: Conversation[] = $state([]);
  let runtimeStatus: RuntimeStatus | null = $state(null);
  let topics: Topic[] = $state([]);
  let pollerStatus: PollerStatus | null = $state(null);
  let ready = $state(false);

  // ── Active selection (persisted to localStorage) ──
  let activeConversationId: string = $state("");
  let activeProviderId: string = $state(localStorage.getItem("agent-app:providerId") ?? "");
  let activeModelId: string = $state(localStorage.getItem("agent-app:modelId") ?? "");

  // ── Messages & loading ──
  let messages: Message[] = $state([]);
  let loading = $state(false);

  // ── UI state ──
  let error = $state("");
  let showCreateModal = $state(false);
  let drawerSidebar = $state(false);
  let drawerInfo = $state(false);

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
  let hasModel = $derived(!!activeProviderId && !!activeModelId);

  // ── Bootstrap ──
  onMount(async () => {
    try {
      const [providersRes, modelsRes, skillsRes, convsRes, statusRes, topicsRes, pollerRes] =
        await Promise.all([
          invoke<ProviderInfo[]>("list_providers"),
          invoke<ModelInfo[]>("list_models"),
          invoke<SkillInfo[]>("list_skills"),
          invoke<Conversation[]>("list_conversations"),
          invoke<RuntimeStatus>("status"),
          invoke<Topic[]>("list_topics"),
          invoke<PollerStatus>("poll_status"),
        ]);

      providers = providersRes;
      models = modelsRes;
      skills = skillsRes;
      conversations = convsRes;
      runtimeStatus = statusRes;
      topics = topicsRes;
      pollerStatus = pollerRes;

      if (convsRes.length > 0) {
        activeConversationId = convsRes[0].id;
      }

      ready = true;
    } catch (e) {
      error = `Failed to load: ${formatInvokeError(e)}`;
    }
  });

  $effect(() => {
    if (!activeConversationId) return;
    invoke<Message[]>("history", { conversationId: activeConversationId })
      .then((msgs) => { messages = msgs; })
      .catch((e) => { error = `Failed to load history: ${formatInvokeError(e)}`; });
  });

  async function handleSend(text: string) {
    if (!activeConversationId) {
      error = "No active session. Create a new session first.";
      return;
    }
    if (!hasModel) {
      error = "Select a provider and model before sending.";
      return;
    }
    const userMsg: Message = { role: "user", content: text, timestamp: Date.now() };
    messages = [...messages, userMsg];
    loading = true;
    error = "";
    try {
      const res = await invoke<ChatResponse>("send_chat_message", {
        message: text, providerId: activeProviderId,
        modelId: activeModelId, conversationId: activeConversationId,
      });
      messages = [...messages, { role: "assistant", content: res.response, timestamp: Date.now() }];
    } catch (e) {
      error = `Send failed: ${formatInvokeError(e)}`;
    } finally {
      loading = false;
    }
  }

  async function handleCreateSession(mode: string) {
    showCreateModal = false;
    try {
      const id = await invoke<string>("create_conversation", { mode });
      conversations = await invoke<Conversation[]>("list_conversations");
      activeConversationId = id;
    } catch (e) {
      error = `Failed to create session: ${formatInvokeError(e)}`;
    }
  }

  async function handleCloseSession(sessionId: string) {
    try {
      await invoke<string>("close_session", { sessionId });
      conversations = conversations.filter((c) => c.id !== sessionId);
      if (activeConversationId === sessionId) {
        activeConversationId = conversations[0]?.id ?? "";
        if (!activeConversationId) messages = [];
      }
    } catch (e) {
      error = `Failed to close session: ${formatInvokeError(e)}`;
    }
  }

  function handleSelectConversation(id: string) {
    activeConversationId = id;
    drawerSidebar = false;
  }

  function handleModelChange(providerId: string, modelId: string) {
    activeProviderId = providerId;
    activeModelId = modelId;
    localStorage.setItem("agent-app:providerId", providerId);
    localStorage.setItem("agent-app:modelId", modelId);
  }

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

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      drawerSidebar = false;
      drawerInfo = false;
      return;
    }
    const ctrl = e.ctrlKey || e.metaKey;
    if (!ctrl) return;
    const key = e.key.toLowerCase();
    e.preventDefault();
    if (key === "j" && e.shiftKey) {
      layoutStore.togglePanel();
    } else if (key === "j") {
      showCreateModal = true;
    } else if (key === "b") {
      layoutStore.toggleSidebar();
    } else if (key === "i") {
      layoutStore.toggleInfo();
    } else if (key === "\\") {
      toggleNeuronSplit();
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
      {providers}
      {models}
      selectedProviderId={activeProviderId}
      selectedModelId={activeModelId}
      neuronActive={isNeuronSplit}
      sidebarVisible={layoutStore.state.sidebar.visible}
      infoVisible={layoutStore.state.info.visible}
      panelVisible={layoutStore.state.panel.visible}
      onChange={handleModelChange}
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
      <SessionList
        {conversations}
        activeId={activeConversationId}
        collapsed={false}
        onSelect={handleSelectConversation}
        onCreate={() => (showCreateModal = true)}
        onClose={handleCloseSession}
        onToggle={() => layoutStore.toggleSidebar()}
      />
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
              <ChatArea {messages} {loading} onSend={handleSend} />
              <Splitter
                orientation="vertical"
                onResize={handleSplitResize}
                onResizeEnd={() => layoutStore.persistNow()}
              />
              <NeuronManager />
            </div>
          {:else if layoutStore.state.activity.active === "neurons"}
            <NeuronManager />
          {:else}
            <ChatArea {messages} {loading} onSend={handleSend} />
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
        <DockPane
          title={panelViews.find((v) => v.id === layoutStore.state.panel.activeView)?.label ?? "Panel"}
          onToggle={() => layoutStore.togglePanel()}
        >
          <div class="panel-tabs">
            {#each panelViews as pv}
              <button
                class="panel-tab"
                class:active={layoutStore.state.panel.activeView === pv.id}
                onclick={() => layoutStore.setPanelView(pv.id)}
              >
                {pv.label}
              </button>
            {/each}
          </div>
          {#if layoutStore.state.panel.activeView === "poller"}
            <PollerPanel bind:pollerStatus />
          {:else if layoutStore.state.panel.activeView === "logs"}
            <LogPanel />
          {:else}
            <p class="panel-empty">Logs placeholder</p>
          {/if}
        </DockPane>
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
      <SidePanel {providers} {models} {skills} {topics} />
    </aside>
  </div>

  <div class="error-area">
    <ErrorBanner message={error} onDismiss={() => (error = "")} />
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
    <SessionList
      {conversations}
      activeId={activeConversationId}
      collapsed={false}
      onSelect={handleSelectConversation}
      onCreate={() => { showCreateModal = true; drawerSidebar = false; }}
      onClose={handleCloseSession}
      onToggle={() => {}}
    />
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
    <SidePanel {providers} {models} {skills} {topics} />
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
    height: 100vh;
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

  .panel-tabs { display: flex; flex-shrink: 0; border-bottom: var(--border-width) solid var(--color-border); padding: 0 var(--space-2); }
  .panel-tab {
    padding: var(--space-1) var(--space-3);
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: var(--fs-sm);
    font-weight: 500;
    color: var(--color-text-muted);
    border-bottom: 2px solid transparent;
  }
  .panel-tab.active { color: var(--color-primary); border-bottom-color: var(--color-primary); }
  .panel-tab:hover { color: var(--color-text); }
  .panel-empty { padding: var(--space-4); color: var(--color-text-muted); font-size: var(--fs-sm); }

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

  .drawer :global(.side-panel) {
    height: 100%;
  }

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

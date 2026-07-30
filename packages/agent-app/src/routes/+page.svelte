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
  } from "$lib/types";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import SessionList from "$lib/components/SessionList.svelte";
  import ChatArea from "$lib/components/ChatArea.svelte";
  import SidePanel from "$lib/components/SidePanel.svelte";
  import SessionCreateModal from "$lib/components/SessionCreateModal.svelte";
  import ErrorBanner from "$lib/components/ErrorBanner.svelte";
  import { locale, t } from "$lib/i18n";
  $locale;

  // ── Bootstrap state (loaded once) ──
  let providers: ProviderInfo[] = $state([]);
  let models: ModelInfo[] = $state([]);
  let skills: SkillInfo[] = $state([]);
  let conversations: Conversation[] = $state([]);
  let runtimeStatus: RuntimeStatus | null = $state(null);
  let ready = $state(false);

  // ── Active selection ──
  let activeConversationId: string = $state("");
  let activeProviderId: string = $state("");
  let activeModelId: string = $state("");

  // ── Messages & loading ──
  let messages: Message[] = $state([]);
  let loading = $state(false);

  // ── UI state ──
  let error = $state("");
  let showCreateModal = $state(false);
  let sidebarCollapsed = $state(false);
  let drawerSidebar = $state(false);
  let drawerInfo = $state(false);

  // ── Derived ──
  let activeConversation = $derived(
    conversations.find((c) => c.id === activeConversationId)
  );
  let activeMode = $derived(activeConversation?.mode ?? "chat");
  let hasModel = $derived(!!activeProviderId && !!activeModelId);

  // ── Bootstrap ──
  onMount(async () => {
    try {
      const [providersRes, modelsRes, skillsRes, convsRes, statusRes] =
        await Promise.all([
          invoke<ProviderInfo[]>("list_providers"),
          invoke<ModelInfo[]>("list_models"),
          invoke<SkillInfo[]>("list_skills"),
          invoke<Conversation[]>("list_conversations"),
          invoke<RuntimeStatus>("status"),
        ]);

      providers = providersRes;
      models = modelsRes;
      skills = skillsRes;
      conversations = convsRes;
      runtimeStatus = statusRes;

      if (convsRes.length > 0) {
        activeConversationId = convsRes[0].id;
      }

      ready = true;
    } catch (e) {
      error = `Failed to load: ${e}`;
    }
  });

  $effect(() => {
    if (!activeConversationId) return;
    invoke<Message[]>("history", { conversationId: activeConversationId })
      .then((msgs) => { messages = msgs; })
      .catch((e) => { error = `Failed to load history: ${e}`; });
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
      error = `Send failed: ${e}`;
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
      error = `Failed to create session: ${e}`;
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
      error = `Failed to close session: ${e}`;
    }
  }

  function handleSelectConversation(id: string) {
    activeConversationId = id;
    drawerSidebar = false;
  }

  function handleModelChange(providerId: string, modelId: string) {
    activeProviderId = providerId;
    activeModelId = modelId;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.ctrlKey && e.key === "j") {
      e.preventDefault();
      showCreateModal = true;
    }
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
  <header class="status-area">
    <StatusBar
      appName={runtimeStatus?.app_name ?? "Agent App"}
      sessionId={activeConversationId}
      mode={activeMode}
      {providers}
      {models}
      selectedProviderId={activeProviderId}
      selectedModelId={activeModelId}
      onChange={handleModelChange}
      onToggleSidebar={() => (drawerSidebar = !drawerSidebar)}
      onToggleInfo={() => (drawerInfo = !drawerInfo)}
    />
  </header>

  <!-- Desktop sidebar -->
  <aside class="sidebar-area desktop-only" class:collapsed={sidebarCollapsed}>
    <SessionList
      {conversations}
      activeId={activeConversationId}
      collapsed={sidebarCollapsed}
      onSelect={handleSelectConversation}
      onCreate={() => (showCreateModal = true)}
      onClose={handleCloseSession}
      onToggle={() => (sidebarCollapsed = !sidebarCollapsed)}
    />
  </aside>

  <main class="chat-area">
    <ChatArea {messages} {loading} onSend={handleSend} />
  </main>

  <!-- Desktop info panel -->
  <aside class="info-area desktop-only">
    <SidePanel {providers} {models} {skills} />
  </aside>

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
    <SidePanel {providers} {models} {skills} />
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

  :global(*) {
    box-sizing: border-box;
  }

  .app-layout {
    display: grid;
    height: 100vh;
    grid-template-rows: auto 1fr auto;
    grid-template-columns: auto 1fr auto;
    grid-template-areas:
      "status status status"
      "sidebar chat info"
      "error error error";
    overflow: hidden;
  }

  .status-area { grid-area: status; }
  .sidebar-area { grid-area: sidebar; }
  .chat-area { grid-area: chat; overflow: hidden; display: flex; flex-direction: column; }
  .info-area { grid-area: info; width: 280px; border-left: var(--border-width) solid var(--color-border); background: var(--color-surface); overflow-y: auto; }
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

  .drawer :global(.side-panel) {
    height: 100%;
  }

  /* ── Responsive: <800px hide desktop panels, show drawers ── */
  @media (max-width: 800px) {
    .desktop-only { display: none; }

    .app-layout {
      grid-template-columns: 1fr;
      grid-template-areas:
        "status"
        "chat"
        "error";
    }

    .info-area.desktop-only { display: none; }
  }

  @media (min-width: 801px) {
    /* On desktop, only show sidebar inline when not in drawer mode */
    .drawer-backdrop,
    .drawer { display: none; }
  }
</style>

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
  import ModelBar from "$lib/components/ModelBar.svelte";
  import SessionList from "$lib/components/SessionList.svelte";
  import ChatArea from "$lib/components/ChatArea.svelte";
  import SidePanel from "$lib/components/SidePanel.svelte";
  import SessionCreateModal from "$lib/components/SessionCreateModal.svelte";
  import ErrorBanner from "$lib/components/ErrorBanner.svelte";

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

  // ── Derived ──
  let activeConversation = $derived(
    conversations.find((c) => c.id === activeConversationId)
  );
  let activeMode = $derived(activeConversation?.mode ?? "chat");
  let hasModel = $derived(!!activeProviderId && !!activeModelId);
  let modelsForProvider = $derived(
    activeProviderId
      ? models.filter((m) => m.provider_id === activeProviderId)
      : []
  );

  // ── Bootstrap ──
  let debugInfo = $state("");

  onMount(async () => {
    try {
      // Debug: check storage path
      const path = await invoke<string>("debug_storage_path").catch(() => "n/a");
      debugInfo = path;

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

      // Auto-select first conversation if available
      if (convsRes.length > 0) {
        activeConversationId = convsRes[0].id;
      }

      ready = true;
    } catch (e) {
      error = `Failed to load: ${e}`;
    }
  });

  // ── Load messages when active conversation changes ──
  $effect(() => {
    if (!activeConversationId) return;

    invoke<Message[]>("history", { conversationId: activeConversationId })
      .then((msgs) => {
        messages = msgs;
      })
      .catch((e) => {
        error = `Failed to load history: ${e}`;
      });
  });

  // ── Send message ──
  async function handleSend(text: string) {
    if (!activeConversationId) {
      error = "No active session. Create a new session first.";
      return;
    }
    if (!hasModel) {
      error = "Select a provider and model before sending.";
      return;
    }

    // Optimistic append
    const userMsg: Message = {
      role: "user",
      content: text,
      timestamp: Date.now(),
    };
    messages = [...messages, userMsg];
    loading = true;
    error = "";

    try {
      const res = await invoke<ChatResponse>("send_chat_message", {
        message: text,
        providerId: activeProviderId,
        modelId: activeModelId,
        conversationId: activeConversationId,
      });

      const assistantMsg: Message = {
        role: "assistant",
        content: res.response,
        timestamp: Date.now(),
      };
      messages = [...messages, assistantMsg];
    } catch (e) {
      error = `Send failed: ${e}`;
    } finally {
      loading = false;
    }
  }

  // ── Create session ──
  async function handleCreateSession(mode: string) {
    showCreateModal = false;
    try {
      const id = await invoke<string>("create_conversation", { mode });
      // Refresh conversation list
      const convs = await invoke<Conversation[]>("list_conversations");
      conversations = convs;
      activeConversationId = id;
    } catch (e) {
      error = `Failed to create session: ${e}`;
    }
  }

  // ── Close session ──
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

  // ── Select conversation ──
  function handleSelectConversation(id: string) {
    activeConversationId = id;
  }

  // ── Model/Provider change ──
  function handleModelChange(providerId: string, modelId: string) {
    activeProviderId = providerId;
    activeModelId = modelId;
  }

  // ── Keyboard shortcuts ──
  function handleKeydown(e: KeyboardEvent) {
    if (e.ctrlKey && e.key === "j") {
      e.preventDefault();
      showCreateModal = true;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="app-layout">
  <header class="status-area">
    <StatusBar
      appName={runtimeStatus?.app_name ?? "Agent App"}
      sessionId={activeConversationId}
      mode={activeMode}
      providerId={activeProviderId}
      modelId={activeModelId}
    />
  </header>

  <nav class="model-area">
    <ModelBar
      {providers}
      totalModels={models.length}
      visibleModels={modelsForProvider}
      selectedProviderId={activeProviderId}
      selectedModelId={activeModelId}
      onChange={handleModelChange}
    />
    <span class="cwd-debug">{debugInfo}</span>
  </nav>

  <aside class="sidebar-area">
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

  <aside class="info-area">
    <SidePanel {providers} {models} {skills} />
  </aside>

  <div class="error-area">
    <ErrorBanner message={error} onDismiss={() => (error = "")} />
  </div>
</div>

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

  :global(:root) {
    --color-bg: #ffffff;
    --color-surface: #f7f8fa;
    --color-text: #1d1d1f;
    --color-text-muted: #86868b;
    --color-border: #e6e6ea;
    --color-primary: #1a73e8;
    --color-on-primary: #ffffff;
    --color-hover: #f0f0f3;
    --color-error: #d93025;
    --color-error-bg: #fce8e6;
    --color-error-border: #f5c6c2;
  }

  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --color-bg: #1c1c1e;
      --color-surface: #2c2c2e;
      --color-text: #f5f5f7;
      --color-text-muted: #98989d;
      --color-border: #38383a;
      --color-primary: #64b5f6;
      --color-on-primary: #1c1c1e;
      --color-hover: #3a3a3c;
      --color-error: #f28b82;
      --color-error-bg: #3c1a1a;
      --color-error-border: #5c2a2a;
    }
  }

  .app-layout {
    display: grid;
    height: 100vh;
    grid-template-rows: auto auto 1fr auto;
    grid-template-columns: auto 1fr auto;
    grid-template-areas:
      "status status status"
      "model model model"
      "sidebar chat info"
      "error error error";
    overflow: hidden;
  }

  .status-area {
    grid-area: status;
  }

  .model-area {
    grid-area: model;
  }

  .sidebar-area {
    grid-area: sidebar;
  }

  .chat-area {
    grid-area: chat;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .info-area {
    grid-area: info;
    width: 280px;
    border-left: 1px solid var(--color-border);
    background: var(--color-surface);
    overflow-y: auto;
  }

  .error-area {
    grid-area: error;
  }

  .loading-overlay {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-bg);
    z-index: 200;
  }

  .loading-overlay p {
    font-size: 16px;
    color: var(--color-text-muted);
  }

  .cwd-debug {
    font-size: 10px;
    color: var(--color-text-muted);
    padding: 4px 12px;
    opacity: 0.7;
    font-family: monospace;
  }

  @media (max-width: 800px) {
    .info-area {
      display: none;
    }
  }
</style>

<script lang="ts">
  import type { Conversation } from "$lib/types";

  let {
    conversations,
    activeId,
    collapsed,
    onSelect,
    onCreate,
    onClose,
    onToggle,
  }: {
    conversations: Conversation[];
    activeId: string;
    collapsed: boolean;
    onSelect: (id: string) => void;
    onCreate: () => void;
    onClose: (id: string) => void;
    onToggle: () => void;
  } = $props();

  const modeLabel: Record<string, string> = {
    chat: "Chat",
    agent: "Agent",
    assistant: "Assistant",
  };

  const modeClass: Record<string, string> = {
    chat: "chat",
    agent: "agent",
    assistant: "assistant",
  };

  function shortId(id: string): string {
    if (id.length <= 16) return id;
    return `${id.slice(0, 8)}..${id.slice(-4)}`;
  }
</script>

<aside class="sidebar" class:collapsed>
  <div class="sidebar-header">
    {#if !collapsed}
      <h2>Sessions</h2>
      <div class="header-actions">
        <button class="icon-btn" onclick={onCreate} title="New session">
          <span class="plus-icon">+</span>
        </button>
        <button class="icon-btn" onclick={onToggle} title="Collapse sidebar">
          <span class="collapse-icon">◀</span>
        </button>
      </div>
    {:else}
      <button class="icon-btn expand-btn" onclick={onToggle} title="Expand sidebar">
        <span>▶</span>
      </button>
      <button class="icon-btn" onclick={onCreate} title="New session">
        <span class="plus-icon">+</span>
      </button>
    {/if}
  </div>

  {#if !collapsed}
    <div class="session-list">
      {#if conversations.length === 0}
        <div class="empty">
          <p>No sessions yet.</p>
          <button class="create-btn" onclick={onCreate}>Create one</button>
        </div>
      {:else}
        {#each conversations as conv}
          <button
            class="session-item"
            class:active={conv.id === activeId}
            onclick={() => onSelect(conv.id)}
          >
            <div class="session-info">
              <span class="session-id">{shortId(conv.id)}</span>
              <span class="session-count">{conv.messages.length} msgs</span>
            </div>
            <div class="session-meta">
              <span class="mode-badge {modeClass[conv.mode] ?? 'chat'}">
                {modeLabel[conv.mode] ?? conv.mode}
              </span>
              {#if conv.mode === "assistant"}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <span
                  class="close-btn"
                  role="button"
                  tabindex="-1"
                  onclick={(e) => { e.stopPropagation(); onClose(conv.id); }}
                  title="Close session"
                >×</span>
              {/if}
            </div>
          </button>
        {/each}
      {/if}
    </div>
  {/if}
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    background: var(--color-surface);
    border-right: 1px solid var(--color-border);
    width: 280px;
    transition: width 0.2s ease;
    overflow: hidden;
  }

  .sidebar.collapsed {
    width: 48px;
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px;
    border-bottom: 1px solid var(--color-border);
    min-height: 48px;
  }

  .sidebar.collapsed .sidebar-header {
    flex-direction: column;
    gap: 8px;
    padding: 8px;
  }

  .sidebar-header h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }

  .header-actions {
    display: flex;
    gap: 4px;
  }

  .icon-btn {
    background: none;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    cursor: pointer;
    padding: 4px 8px;
    font-size: 14px;
    color: var(--color-text);
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 28px;
    min-height: 28px;
    transition: background 0.15s;
  }

  .icon-btn:hover {
    background: var(--color-hover);
  }

  .expand-btn {
    writing-mode: vertical-lr;
  }

  .plus-icon {
    font-weight: 700;
    font-size: 16px;
  }

  .session-list {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
  }

  .empty {
    text-align: center;
    padding: 24px 8px;
    color: var(--color-text-muted);
    font-size: 13px;
  }

  .create-btn {
    margin-top: 8px;
    padding: 6px 16px;
    border-radius: 6px;
    border: 1px solid var(--color-primary);
    background: transparent;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 13px;
  }

  .create-btn:hover {
    background: var(--color-primary);
    color: var(--color-on-primary);
  }

  .session-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 8px 10px;
    margin-bottom: 4px;
    border-radius: 8px;
    border: none;
    background: transparent;
    cursor: pointer;
    text-align: left;
    transition: background 0.15s;
    color: var(--color-text);
  }

  .session-item:hover {
    background: var(--color-hover);
  }

  .session-item.active {
    background: var(--color-primary);
    color: var(--color-on-primary);
  }

  .session-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .session-id {
    font-size: 13px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .session-count {
    font-size: 11px;
    opacity: 0.6;
  }

  .session-meta {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .mode-badge {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    padding: 2px 6px;
    border-radius: 4px;
    letter-spacing: 0.03em;
  }

  .mode-badge.chat {
    background: #e8f4fd;
    color: #1a73e8;
  }

  .mode-badge.agent {
    background: #e6f7e6;
    color: #1a8a1a;
  }

  .mode-badge.assistant {
    background: #f3e8fd;
    color: #7c3aed;
  }

  :global(.dark) .mode-badge.chat {
    background: #1a3a5c;
    color: #64b5f6;
  }

  :global(.dark) .mode-badge.agent {
    background: #1a3a1a;
    color: #66bb6a;
  }

  :global(.dark) .mode-badge.assistant {
    background: #2a1a3a;
    color: #b39ddb;
  }

  .close-btn {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 16px;
    color: inherit;
    opacity: 0;
    padding: 2px 4px;
    border-radius: 4px;
    line-height: 1;
    transition: opacity 0.15s;
  }

  .session-item:hover .close-btn {
    opacity: 0.6;
  }

  .close-btn:hover {
    opacity: 1 !important;
    background: rgba(0, 0, 0, 0.1);
  }
</style>

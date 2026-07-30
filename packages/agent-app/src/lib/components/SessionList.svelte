<script lang="ts">
  import type { Conversation } from "$lib/types";
  import { t } from "$lib/i18n";

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

  function shortId(id: string): string {
    if (id.length <= 16) return id;
    return `${id.slice(0, 8)}..${id.slice(-4)}`;
  }

  function formatTime(ts: number): string {
    const d = new Date(ts);
    const h = d.getHours().toString().padStart(2, "0");
    const m = d.getMinutes().toString().padStart(2, "0");
    return `${h}:${m}`;
  }
</script>

<aside class="sidebar" class:collapsed>
  <div class="sidebar-header">
    {#if !collapsed}
      <h2>{t("sessionList.title")}</h2>
      <div class="header-actions">
        <button class="icon-btn" onclick={onCreate} title={t("sessionList.create")}>
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
      <button class="icon-btn" onclick={onCreate} title={t("sessionList.create")}>
        <span class="plus-icon">+</span>
      </button>
    {/if}
  </div>

  {#if !collapsed}
    <div class="session-list">
      {#if conversations.length === 0}
        <div class="empty">
          <p>{t("sessionList.empty")}</p>
          <button class="create-btn" onclick={onCreate}>{t("sessionList.create")}</button>
        </div>
      {:else}
        {#each conversations as conv}
          <button
            class="session-item"
            class:active={conv.id === activeId}
            onclick={() => onSelect(conv.id)}
          >
            <div class="session-indicator" class:active={conv.id === activeId}>
              <span class="dot">●</span>
            </div>
            <div class="session-info">
              <span class="session-id">{shortId(conv.id)}</span>
              <span class="session-meta">
                <span class="mode-badge {conv.mode}">{modeLabel[conv.mode] ?? conv.mode}</span>
                <span class="session-count">{conv.messages.length} {t("sessionList.msgs")}</span>
                <span class="session-time" title={new Date(conv.updated_at).toLocaleString()}>
                  {formatTime(conv.updated_at)}
                </span>
              </span>
            </div>
            <div class="session-actions">
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
  .sidebar { display: flex; flex-direction: column; background: var(--color-surface); border-right: var(--border-width) solid var(--color-border); width: 260px; height: 100%; transition: width var(--duration-fast) var(--ease-out); overflow: hidden; z-index: 1; box-shadow: 2px 0 8px rgba(0,0,0,0.05); }
  .sidebar.collapsed { width: 48px; }
  .sidebar-header { display: flex; align-items: center; justify-content: space-between; padding: var(--space-3); border-bottom: var(--border-width) solid var(--color-border); min-height: 48px; }
  .sidebar.collapsed .sidebar-header { flex-direction: column; gap: var(--space-2); padding: var(--space-2); }
  .sidebar-header h2 { margin: 0; font-size: var(--fs-base); font-weight: 600; }
  .header-actions { display: flex; gap: var(--space-1); }
  .icon-btn { background: none; border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); cursor: pointer; padding: var(--space-1) var(--space-2); font-size: var(--fs-base); color: var(--color-text); display: flex; align-items: center; justify-content: center; min-width: 28px; min-height: 28px; transition: background var(--duration-fast) var(--ease-out); }
  .icon-btn:hover { background: var(--color-hover); }
  .expand-btn { writing-mode: vertical-lr; }
  .plus-icon { font-weight: 700; font-size: 16px; }
  .session-list { flex: 1; overflow-y: auto; padding: var(--space-2); }
  .empty { text-align: center; padding: var(--space-6) var(--space-2); color: var(--color-text-muted); font-size: var(--fs-sm); }
  .create-btn { margin-top: var(--space-2); padding: var(--space-1) var(--space-4); border-radius: var(--radius-sm); border: var(--border-width) solid var(--color-primary); background: transparent; color: var(--color-primary); cursor: pointer; font-size: var(--fs-sm); }
  .create-btn:hover { background: var(--color-primary); color: var(--color-on-primary); }
  .session-item { display: flex; align-items: center; gap: var(--space-2); width: 100%; padding: var(--space-2) var(--space-2); margin-bottom: 2px; border-radius: var(--radius-md); border: none; background: transparent; cursor: pointer; text-align: left; transition: background var(--duration-fast) var(--ease-out); color: var(--color-text); }
  .session-item:hover { background: var(--color-hover); }
  .session-item.active { background: var(--color-hover); }
  .session-indicator { flex-shrink: 0; width: 12px; display: flex; align-items: center; }
  .dot { font-size: 10px; color: transparent; transition: color var(--duration-fast) var(--ease-out); }
  .session-indicator.active .dot { color: var(--color-primary); }
  .session-info { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .session-id { font-size: var(--fs-sm); font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .session-meta { display: flex; align-items: center; gap: var(--space-2); font-size: var(--fs-xs); color: var(--color-text-muted); }
  .mode-badge { font-size: 10px; font-weight: 600; text-transform: uppercase; padding: 1px 5px; border-radius: var(--radius-sm); letter-spacing: 0.03em; }
  .mode-badge.chat { background: color-mix(in srgb, var(--color-primary) 15%, transparent); color: var(--color-primary); }
  .mode-badge.agent { background: color-mix(in srgb, var(--color-success) 15%, transparent); color: var(--color-success); }
  .mode-badge.assistant { background: color-mix(in srgb, var(--color-warning) 15%, transparent); color: var(--color-warning); }
  .session-time { margin-left: auto; }
  .session-actions { flex-shrink: 0; }
  .close-btn { background: none; border: none; cursor: pointer; font-size: 16px; color: inherit; opacity: 0; padding: 2px 4px; border-radius: var(--radius-sm); line-height: 1; transition: opacity var(--duration-fast) var(--ease-out); }
  .session-item:hover .close-btn { opacity: 0.6; }
  .close-btn:hover { opacity: 1 !important; background: rgba(0, 0, 0, 0.1); }
</style>

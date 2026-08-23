<script lang="ts">
  import { t } from "$lib/i18n";
  import { useViewContext } from "$lib/layout/viewContext";
  import type { Conversation } from "$lib/types";
  import { CopyToClipboard } from "$lib/utils";

  // 数据/命令统一来自 ViewContext；collapsed 是纯视觉 prop（窄侧栏形态）。
  const ctx = useViewContext();
  let { collapsed = false }: { collapsed?: boolean } = $props();

  let activeId = $derived(ctx.stores.data.state.activeConversationId ?? "");
  let conversations = $derived(ctx.stores.data.state.conversations);
  let runningSessionIds = $derived(
    new Set(ctx.stores.data.state.runningSessions.map((s) => s.session_id)),
  );

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

  // 会话标题：取首条 user/assistant 文本消息，无则显示占位。
  function sessionTitle(conv: Conversation): string {
    const textMsg = conv.messages.find(
      (m) => m.body.kind === "text" && (m.role === "user" || m.role === "assistant"),
    );
    const content = textMsg?.body.kind === "text" ? textMsg.body.content.trim() : "";
    return content || t("sessionList.newSession");
  }
</script>

<aside class="sidebar" class:collapsed>
  <div class="sidebar-header">
    {#if !collapsed}
      <h2>{t("sessionList.title")}</h2>
      <div class="header-actions">
        <button class="icon-btn" onclick={onCreate} title={t("sessionList.newButton")}>
          <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
        </button>
        <button class="icon-btn" onclick={onToggle} title={t("sessionList.collapseSidebar")}>
          <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
        </button>
      </div>
    {:else}
      <button class="icon-btn expand-btn" onclick={onToggle} title={t("sessionList.expandSidebar")}>
        <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
      </button>
      <button class="icon-btn" onclick={onCreate} title={t("sessionList.newButton")}>
        <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
      </button>
    {/if}
  </div>

  {#if !collapsed}
    <div class="session-list">
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
                <span class="session-count">{conv.messages.length} {t("sessionList.msgs")}</span>
                <span class="session-time" title={new Date(conv.updated_at).toLocaleString()}>
                  {formatTime(conv.updated_at)}
                </span>
              </span>
            </div>
            <div class="session-actions">
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
  .session-list { flex: 1; overflow-y: auto; padding: 0; }
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
    .close-btn {
      opacity: 0;
      visibility: hidden;
    }
    .session-item:hover .copy-btn,
    .session-item:focus-within .copy-btn,
    .session-item:hover .close-btn,
    .session-item:focus-within .close-btn {
      opacity: 0.6;
      visibility: visible;
    }
  }
</style>

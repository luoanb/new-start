<script lang="ts">
  let { appName, sessionId, mode, providerId, modelId }: {
    appName: string;
    sessionId: string;
    mode: string;
    providerId: string;
    modelId: string;
  } = $props();

  function shortId(id: string): string {
    if (id.length <= 16) return id;
    return `${id.slice(0, 8)}..${id.slice(-4)}`;
  }

  const modeLabel: Record<string, string> = {
    chat: "Chat",
    agent: "Agent",
    assistant: "Assistant",
  };
</script>

<header class="status-bar">
  <div class="bar-left">
    <span class="app-name">{appName}</span>
  </div>
  <div class="bar-center">
    {#if sessionId}
      <span class="session-info">
        Session: <strong>{shortId(sessionId)}</strong>
      </span>
      <span class="mode-tag">{modeLabel[mode] ?? mode}</span>
    {/if}
  </div>
  <div class="bar-right">
    {#if providerId}
      <span class="model-info">
        {providerId}/{modelId}
      </span>
    {:else}
      <span class="model-info no-model">No model selected</span>
    {/if}
  </div>
</header>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 16px;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
    font-size: 13px;
    min-height: 36px;
  }

  .bar-left,
  .bar-center,
  .bar-right {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .bar-center {
    flex: 1;
    justify-content: center;
  }

  .app-name {
    font-weight: 600;
    font-size: 14px;
  }

  .session-info {
    color: var(--color-text-muted);
  }

  .mode-tag {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    padding: 2px 8px;
    border-radius: 4px;
    background: var(--color-primary);
    color: var(--color-on-primary);
    letter-spacing: 0.03em;
  }

  .model-info {
    color: var(--color-text-muted);
    font-family: monospace;
    font-size: 12px;
  }

  .no-model {
    color: var(--color-error);
  }
</style>

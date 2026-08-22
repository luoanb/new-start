<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api, c } from "$lib/api";
  import { STATE_CHANGED_EVENT } from "$lib/api/types";
  import { t, tMap } from "$lib/i18n";
  import Select from "./Select.svelte";
  import Toggle from "./Toggle.svelte";
  import Tooltip from "./Tooltip.svelte";
  import type {
    McpServerStatus,
    ToolInfo,
  } from "$lib/types";
  import type { StateChangePayload } from "$lib/api/types";
  import { useViewContext } from "$lib/layout/viewContext";

  const ctx = useViewContext();

  let tools = $state<ToolInfo[]>([]);
  let mcpServers = $state<McpServerStatus[]>([]);
  let loading = $state(true);
  let refreshing = $state(false);
  let errorMsg = $state("");
  let unlisten: (() => void) | null = null;

  onMount(async () => {
    await refresh();
    // 启动后台装配 / 刷新 / 保存配置都会广播 Tools 事件，面板自动跟随。
    unlisten = api.subscribe((payload) => {
      if (payload.kind === "tools") {
        void refresh(true);
      }
    });
  });

  onDestroy(() => {
    unlisten?.();
  });

  async function refresh(silent = false) {
    if (!silent) loading = true;
    errorMsg = "";
    try {
      const [toolList, serverList] = await Promise.all([
        api.call(c.listTools, undefined),
        api.call(c.listMcpServers, undefined),
      ]);
      tools = toolList;
      mcpServers = serverList;
    } catch (e) {
      errorMsg = t("toolPanel.loadListFailed", { error: `${e}` });
    } finally {
      loading = false;
    }
  }

  /// 手动刷新：重新读取磁盘配置并全量重装配（不打开弹窗、不写文件）。
  async function handleRefresh() {
    if (refreshing) return;
    refreshing = true;
    errorMsg = "";
    try {
      await api.call(c.reassembleTools, undefined);
      await refresh(true);
    } catch (e) {
      errorMsg = t("toolPanel.reassembleFailed", { error: `${e}` });
    } finally {
      refreshing = false;
    }
  }

  function sourceLabel(source: ToolInfo["source"]): string {
    switch (source) {
      case "native":
        return "native";
      case "config":
        return "config";
      case "mcp":
        return "mcp";
    }
  }

  /** 打开配置编辑器：在 main 区新建独立面板（与对话同级）。 */
  function openEditor() {
    ctx.commands.openToolEditor();
  }
</script>

<div class="tools-panel">
  {#if errorMsg}
    <button class="error-banner" type="button" onclick={() => (errorMsg = "")}>
      <span class="error-text">{errorMsg}</span>
      <span class="error-dismiss" aria-hidden="true">×</span>
    </button>
  {/if}

  <div class="panel-toolbar">
    <span class="panel-title">{t("toolPanel.title")}</span>
    <div class="toolbar-actions">
      <Tooltip label={t("toolPanel.reload")}>
        <button class="icon-btn" class:spinning={refreshing} type="button" onclick={handleRefresh} disabled={loading || refreshing} aria-label={t("toolPanel.reload")}>
          <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 12a9 9 0 1 1-2.64-6.36" />
            <path d="M21 3v6h-6" />
          </svg>
        </button>
      </Tooltip>
      <Tooltip label={t("toolPanel.editConfig")}>
        <button class="icon-btn" type="button" onclick={openEditor} aria-label={t("toolPanel.editConfig")}>
          <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M17 3a2.83 2.83 0 0 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
            <path d="m15 5 4 4" />
          </svg>
        </button>
      </Tooltip>
    </div>
  </div>

  {#if loading}
    <p class="empty">{t("toolPanel.loading")}</p>
  {:else}
    <section class="section">
      <div class="section-header">
        <span class="section-title">{t("toolPanel.mcpSection")}</span>
        <span class="section-count">{mcpServers.length}</span>
      </div>
      {#if mcpServers.length === 0}
        <p class="empty">{t("toolPanel.noMcpServers")}</p>
      {:else}
        <ul class="server-list">
          {#each mcpServers as server (server.name)}
            <li class="server-item">
              <div class="server-main">
                <span class="server-name">{server.name}</span>
                <span class="server-meta">
                  <span class="transport">{server.transport}</span>
                  <span class="tool-count">{t("toolPanel.toolsCount", { count: server.tool_count })}</span>
                </span>
              </div>
              <span
                class="status"
                class:connecting={server.status === "connecting"}
                class:connected={server.status === "connected"}
                class:failed={server.status === "failed"}
                class:disabled={server.status === "disabled"}
              >
                {tMap("toolPanel.status", server.status)}
              </span>
            </li>
            {#if server.error}
              <li class="server-error" title={server.error}>{server.error}</li>
            {/if}
          {/each}
        </ul>
      {/if}
    </section>

    <section class="section">
      <div class="section-header">
        <span class="section-title">{t("toolPanel.toolsSection")}</span>
        <span class="section-count">{tools.length}</span>
      </div>
      {#if tools.length === 0}
        <p class="empty">{t("toolPanel.noTools")}</p>
      {:else}
        <ul class="tool-list">
          {#each tools as tool (tool.name)}
            <li class="tool-item">
              <div class="tool-main">
                <span class="tool-name" title={tool.name}>{tool.name}</span>
                <span class="tool-desc" title={tool.description}>{tool.description}</span>
              </div>
              {#if tool.tag !== "normal"}
                <span
                  class="tag"
                  class:tag-core={tool.tag === "core"}
                  class:tag-system={tool.tag === "system"}
                >
                  {tool.tag}
                </span>
              {/if}
              <span class="source" class:src-native={tool.source === "native"} class:src-config={tool.source === "config"} class:src-mcp={tool.source === "mcp"}>
                {sourceLabel(tool.source)}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

<style>
  .tools-panel {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: var(--space-2);
    padding: var(--space-2);
    overflow: auto;
  }

  .panel-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .panel-title {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--color-text);
  }
  .toolbar-actions {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .icon-btn {
    width: 26px;
    height: 26px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: background var(--duration-fast) var(--ease-out),
                color var(--duration-fast) var(--ease-out);
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--color-hover);
    color: var(--color-text);
  }
  .icon-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .icon-btn .icon {
    display: block;
  }
  .icon-btn.spinning .icon {
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    width: 100%;
    text-align: left;
    border: none;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--color-error-bg);
    color: var(--color-error);
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .error-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .error-dismiss {
    flex-shrink: 0;
    font-weight: 600;
    opacity: 0.7;
  }

  .empty {
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--fs-sm);
    padding: var(--space-4) 0;
  }

  /* ── Sections（行式列表，无卡片堆叠）── */
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .section-header {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    padding-bottom: var(--space-1);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .section-title {
    font-size: var(--fs-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }
  .section-count {
    font-family: monospace;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }

  .server-list,
  .tool-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .server-item,
  .tool-item {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-2);
    border-radius: var(--radius-sm);
    transition: background var(--duration-fast) var(--ease-out);
  }
  .server-item:hover,
  .tool-item:hover {
    background: var(--color-hover);
  }

  .server-main,
  .tool-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .server-name,
  .tool-name {
    font-size: var(--fs-sm);
    color: var(--color-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .server-meta {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .tool-desc {
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* 状态与来源：语义色文字，克制配色 */
  .status {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    flex-shrink: 0;
  }
  .status.connected { color: var(--color-success); }
  .status.connecting { color: var(--color-warning); }
  .status.failed { color: var(--color-error); }
  .status.disabled { opacity: 0.6; }

  .source {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    flex-shrink: 0;
  }
  /* 标签（core / system；normal 不显式显示）：弱化展示，仅文字区分 */
  .tag {
    display: inline-flex;
    align-items: center;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    flex-shrink: 0;
  }
  .tag-core { color: var(--color-primary); }
  .tag-system { color: var(--color-warning); }

  .server-error {
    list-style: none;
    margin: 0;
    padding: 0 var(--space-2) var(--space-1);
    font-size: var(--fs-xs);
    color: var(--color-error);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>

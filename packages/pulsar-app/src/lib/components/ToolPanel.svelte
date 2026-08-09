<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { STATE_CHANGED_EVENT } from "$lib/stores/dataStore.svelte";
  import { t, tMap } from "$lib/i18n";
  import Select from "./Select.svelte";
  import Toggle from "./Toggle.svelte";
  import Tooltip from "./Tooltip.svelte";
  import type {
    CommandToolConfig,
    HttpToolConfig,
    McpServerConfig,
    McpServerStatus,
    ToolConfigView,
    ToolInfo,
  } from "$lib/types";
  import type { StateChangePayload } from "$lib/stores/dataStore.svelte";

  let tools = $state<ToolInfo[]>([]);
  let mcpServers = $state<McpServerStatus[]>([]);
  let loading = $state(true);
  let refreshing = $state(false);
  let errorMsg = $state("");
  let unlisten: UnlistenFn | null = null;

  // 配置编辑器状态（列表只读展示不变，编辑收敛在弹窗）
  let editorOpen = $state(false);
  let editorLoading = $state(false);
  let editorError = $state("");
  let saving = $state(false);
  let draft = $state<ToolConfigView>({
    mcp_servers: [],
    http_tools: [],
    command_tools: [],
  });

  onMount(async () => {
    await refresh();
    // 启动后台装配 / 刷新 / 保存配置都会广播 Tools 事件，面板自动跟随。
    unlisten = await listen<StateChangePayload>(STATE_CHANGED_EVENT, (event) => {
      if (event.payload.kind === "tools") {
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
        invoke<ToolInfo[]>("list_tools"),
        invoke<McpServerStatus[]>("list_mcp_servers"),
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
      await invoke("reassemble_tools");
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

  async function openEditor() {
    editorError = "";
    editorLoading = true;
    editorOpen = true;
    try {
      const view = await invoke<ToolConfigView>("get_tool_config");
      draft = {
        mcp_servers: view.mcp_servers.map((s) => ({ ...s })),
        http_tools: view.http_tools.map((h) => ({ ...h })),
        command_tools: view.command_tools.map((c) => ({ ...c })),
      };
    } catch (e) {
      editorError = t("toolPanel.loadFailed", { error: `${e}` });
    } finally {
      editorLoading = false;
    }
  }

  function closeEditor() {
    if (saving) return;
    editorOpen = false;
    editorError = "";
  }

  function argsText(args?: string[]): string {
    return args ? args.join(", ") : "";
  }

  function updateArgs(server: McpServerConfig, value: string) {
    server.args = value
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
  }

  async function saveConfig() {
    saving = true;
    editorError = "";
    try {
      const view = await invoke<ToolConfigView>("save_tool_config", {
        view: {
          mcp_servers: draft.mcp_servers,
          http_tools: draft.http_tools,
          command_tools: draft.command_tools,
        },
      });
      draft = view;
      editorOpen = false;
      await refresh();
    } catch (e) {
      editorError = `${e}`;
    } finally {
      saving = false;
    }
  }

  function newMcpServer(): McpServerConfig {
    return {
      name: "",
      transport: "stdio",
      command: "",
      args: [],
      env: {},
      url: "",
      headers: {},
      disabled: false,
    };
  }

  function newHttpTool(): HttpToolConfig {
    return { name: "", desc: "", method: "GET", url: "", timeout_ms: null };
  }

  function newCommandTool(): CommandToolConfig {
    return { name: "", desc: "", template: "", timeout_ms: null };
  }

  // 弹窗滚动防护：滚动链只发生在 modal-body 内部，不会穿透滚到背景页面。
  function preventOverlayScroll(e: WheelEvent) {
    const target = e.target;
    if (target instanceof Element && target.closest(".modal")) return;
    e.preventDefault();
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
                <span class="status-dot" aria-hidden="true"></span>
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
                <span class="tool-name">{tool.name}</span>
                <span class="tool-desc">{tool.description}</span>
              </div>
              <span class="source" class:src-native={tool.source === "native"} class:src-config={tool.source === "config"} class:src-mcp={tool.source === "mcp"}>
                <span class="source-dot" aria-hidden="true"></span>
                {sourceLabel(tool.source)}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

{#if editorOpen}
  <div class="modal-overlay" role="presentation" onwheel={preventOverlayScroll}>
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-label={t("toolPanel.modalAria")}
      tabindex="-1"
    >
      <div class="modal-header">
        <span class="modal-title">{t("toolPanel.modalTitle")}</span>
        <Tooltip label={t("toolPanel.close")}>
          <button class="icon-btn" type="button" onclick={closeEditor} disabled={saving} aria-label={t("toolPanel.close")}>
            <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </Tooltip>
      </div>

      {#if editorError}
        <button class="error-banner" type="button" onclick={() => (editorError = "")}>
          <span class="error-text">{editorError}</span>
          <span class="error-dismiss" aria-hidden="true">×</span>
        </button>
      {/if}

      {#if editorLoading}
        <p class="empty">{t("toolPanel.loadingConfig")}</p>
      {:else}
        <div class="modal-body">
          <!-- MCP Servers -->
          <div class="editor-group">
            <div class="editor-group-header">
              <span>{t("toolPanel.mcpSection")}</span>
              <Tooltip label={t("toolPanel.add")}>
                <button class="icon-btn" type="button" onclick={() => draft.mcp_servers.push(newMcpServer())} aria-label={t("toolPanel.add")}>
                  <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <line x1="12" y1="5" x2="12" y2="19" />
                    <line x1="5" y1="12" x2="19" y2="12" />
                  </svg>
                </button>
              </Tooltip>
            </div>
            {#if draft.mcp_servers.length === 0}
              <p class="empty">{t("toolPanel.emptyMcp")}</p>
            {:else}
              {#each draft.mcp_servers as server, i (i)}
                <div class="editor-item">
                  <div class="field-row">
                    <label class="field">
                      <span class="field-label">{t("toolPanel.name")}</span>
                      <input type="text" bind:value={server.name} placeholder="filesystem" />
                    </label>
                    <label class="field">
                      <span class="field-label">{t("toolPanel.transport")}</span>
                      <Select
                        value={server.transport}
                        options={[
                          { value: "stdio", label: "stdio" },
                          { value: "http", label: "http" },
                        ]}
                        onchange={(v) => (server.transport = v as "stdio" | "http")}
                      />
                    </label>
                    <div class="field-toggle">
                      <Toggle bind:checked={server.disabled} label={t("toolPanel.disabled")} />
                    </div>
                  </div>
                  {#if server.transport === "stdio"}
                    <label class="field">
                      <span class="field-label">{t("toolPanel.command")}</span>
                      <input type="text" bind:value={server.command} placeholder="npx" />
                    </label>
                    <label class="field">
                      <span class="field-label">{t("toolPanel.args")}</span>
                      <input
                        type="text"
                        value={argsText(server.args)}
                        onchange={(e) => updateArgs(server, e.currentTarget.value)}
                        placeholder="-y, @modelcontextprotocol/server-filesystem, /tmp"
                      />
                    </label>
                  {:else}
                    <label class="field">
                      <span class="field-label">{t("toolPanel.url")}</span>
                      <input type="text" bind:value={server.url} placeholder="http://127.0.0.1:8000/mcp" />
                    </label>
                  {/if}
                  <div class="editor-item-footer">
                    <span class="editor-item-hint">{t("toolPanel.transportHint")}</span>
                    <Tooltip label={t("toolPanel.delete")} position="top">
                      <button class="icon-btn danger" type="button" onclick={() => draft.mcp_servers.splice(i, 1)} aria-label={t("toolPanel.delete")}>
                        <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                          <polyline points="3 6 5 6 21 6" />
                          <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                          <line x1="10" y1="11" x2="10" y2="17" />
                          <line x1="14" y1="11" x2="14" y2="17" />
                        </svg>
                      </button>
                    </Tooltip>
                  </div>
                </div>
              {/each}
            {/if}
          </div>

          <!-- HTTP Tools -->
          <div class="editor-group">
            <div class="editor-group-header">
              <span>{t("toolPanel.httpToolsSection")}</span>
              <Tooltip label={t("toolPanel.add")}>
                <button class="icon-btn" type="button" onclick={() => draft.http_tools.push(newHttpTool())} aria-label={t("toolPanel.add")}>
                  <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <line x1="12" y1="5" x2="12" y2="19" />
                    <line x1="5" y1="12" x2="19" y2="12" />
                  </svg>
                </button>
              </Tooltip>
            </div>
            {#if draft.http_tools.length === 0}
              <p class="empty">{t("toolPanel.emptyHttp")}</p>
            {:else}
              {#each draft.http_tools as tool, i (i)}
                <div class="editor-item">
                  <div class="field-row">
                    <label class="field">
                      <span class="field-label">{t("toolPanel.name")}</span>
                      <input type="text" bind:value={tool.name} placeholder="lookup_wiki" />
                    </label>
                    <label class="field">
                      <span class="field-label">{t("toolPanel.method")}</span>
                      <Select
                        value={tool.method ?? "GET"}
                        options={[
                          { value: "GET", label: "GET" },
                          { value: "POST", label: "POST" },
                          { value: "PUT", label: "PUT" },
                          { value: "DELETE", label: "DELETE" },
                        ]}
                        onchange={(v) => (tool.method = String(v))}
                      />
                    </label>
                    <label class="field field-narrow">
                      <span class="field-label">{t("toolPanel.timeoutMs")}</span>
                      <input type="number" bind:value={tool.timeout_ms} placeholder={t("toolPanel.optional")} />
                    </label>
                  </div>
                  <label class="field">
                    <span class="field-label">{t("toolPanel.desc")}</span>
                    <input type="text" bind:value={tool.desc} placeholder={t("toolPanel.descPlaceholder")} />
                  </label>
                  <label class="field">
                    <span class="field-label">{t("toolPanel.url")}</span>
                    <input type="text" bind:value={tool.url} placeholder={"https://api.example.com/wiki?q={query}"} />
                  </label>
                  <div class="editor-item-footer">
                    <span class="editor-item-hint">{t("toolPanel.httpUrlHint")}</span>
                    <Tooltip label={t("toolPanel.delete")} position="top">
                      <button class="icon-btn danger" type="button" onclick={() => draft.http_tools.splice(i, 1)} aria-label={t("toolPanel.delete")}>
                        <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                          <polyline points="3 6 5 6 21 6" />
                          <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                          <line x1="10" y1="11" x2="10" y2="17" />
                          <line x1="14" y1="11" x2="14" y2="17" />
                        </svg>
                      </button>
                    </Tooltip>
                  </div>
                </div>
              {/each}
            {/if}
          </div>

          <!-- Command Tools -->
          <div class="editor-group">
            <div class="editor-group-header">
              <span>{t("toolPanel.commandToolsSection")}</span>
              <Tooltip label={t("toolPanel.add")}>
                <button class="icon-btn" type="button" onclick={() => draft.command_tools.push(newCommandTool())} aria-label={t("toolPanel.add")}>
                  <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <line x1="12" y1="5" x2="12" y2="19" />
                    <line x1="5" y1="12" x2="19" y2="12" />
                  </svg>
                </button>
              </Tooltip>
            </div>
            {#if draft.command_tools.length === 0}
              <p class="empty">{t("toolPanel.emptyCommand")}</p>
            {:else}
              {#each draft.command_tools as tool, i (i)}
                <div class="editor-item">
                  <div class="field-row">
                    <label class="field">
                      <span class="field-label">{t("toolPanel.name")}</span>
                      <input type="text" bind:value={tool.name} placeholder="git_status" />
                    </label>
                    <label class="field field-narrow">
                      <span class="field-label">{t("toolPanel.timeoutMs")}</span>
                      <input type="number" bind:value={tool.timeout_ms} placeholder={t("toolPanel.optional")} />
                    </label>
                  </div>
                  <label class="field">
                    <span class="field-label">{t("toolPanel.desc")}</span>
                    <input type="text" bind:value={tool.desc} placeholder={t("toolPanel.descPlaceholder")} />
                  </label>
                  <label class="field">
                    <span class="field-label">{t("toolPanel.template")}</span>
                    <textarea bind:value={tool.template} rows="2" placeholder="git status --porcelain"></textarea>
                  </label>
                  <div class="editor-item-footer">
                    <span class="editor-item-hint">{t("toolPanel.commandHint")}</span>
                    <Tooltip label={t("toolPanel.delete")} position="top">
                      <button class="icon-btn danger" type="button" onclick={() => draft.command_tools.splice(i, 1)} aria-label={t("toolPanel.delete")}>
                        <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                          <polyline points="3 6 5 6 21 6" />
                          <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                          <line x1="10" y1="11" x2="10" y2="17" />
                          <line x1="14" y1="11" x2="14" y2="17" />
                        </svg>
                      </button>
                    </Tooltip>
                  </div>
                </div>
              {/each}
            {/if}
          </div>
        </div>

        <div class="modal-footer">
          <span class="hint">{t("toolPanel.saveHint")}</span>
          <button class="btn" type="button" onclick={closeEditor} disabled={saving}>{t("toolPanel.cancel")}</button>
          <button class="btn primary" type="button" onclick={saveConfig} disabled={saving || editorLoading}>
            {saving ? t("toolPanel.saving") : t("toolPanel.save")}
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .tools-panel {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: var(--space-6);
    padding: var(--space-3) var(--space-4);
    overflow: auto;
  }

  .panel-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .panel-title {
    font-size: var(--fs-base);
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
  .icon-btn.danger {
    color: var(--color-error);
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
    border-radius: var(--radius-md);
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
    letter-spacing: 0.08em;
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
    font-family: monospace;
    font-size: var(--fs-sm);
    font-weight: 600;
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

  /* 状态与来源：小圆点 + 文本，克制配色 */
  .status {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--fs-xs);
    font-weight: 500;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }
  .status-dot,
  .source-dot {
    width: 7px;
    height: 7px;
    border-radius: var(--radius-full);
    background: var(--color-text-muted);
  }
  .status.connected .status-dot { background: var(--color-success); }
  .status.connecting .status-dot {
    background: var(--color-warning);
    animation: status-pulse 1.2s ease-in-out infinite;
  }
  .status.connecting { color: var(--color-warning); }
  .status.failed .status-dot { background: var(--color-error); }
  .status.failed { color: var(--color-error); }
  .status.disabled { opacity: 0.6; }
  @keyframes status-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.35; }
  }

  .source {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--fs-xs);
    font-weight: 500;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }
  .source.src-native .source-dot { background: var(--color-primary); }
  .source.src-config .source-dot { background: var(--color-warning); }
  .source.src-mcp .source-dot { background: var(--color-success); }

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

  /* ── 弹窗 ── */
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    animation: fade-in var(--duration-normal) var(--ease-out);
  }
  .modal {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    width: min(640px, 92vw);
    max-height: 82vh;
    background: var(--color-elevated);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-lg);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.18);
    padding: var(--space-4);
    animation: modal-in var(--duration-normal) var(--ease-out);
  }
  @keyframes fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }
  @keyframes modal-in {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .modal-title {
    font-size: var(--fs-lg);
    font-weight: 600;
    color: var(--color-text);
  }

  .modal-body {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: var(--space-6);
    overflow: auto;
    overscroll-behavior: contain;
    padding: 2px;
  }

  /* 编辑分组：单一容器 + 条目分隔线，避免嵌套卡片 */
  .editor-group {
    display: flex;
    flex-direction: column;
    /* overflow:hidden 使 min-height:auto 失效（自动最小尺寸=0），
       不加 flex-shrink:0 会在空间不足时被压缩并裁切内容，
       导致 modal-body 无溢出、不出滚动条、滚轮穿透背景。 */
    flex-shrink: 0;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    overflow: hidden;
  }
  .editor-group-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-2) var(--space-3);
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--color-text);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .editor-item {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .editor-item:last-of-type {
    border-bottom: none;
  }
  .editor-item:hover {
    background: var(--color-hover);
  }

  .field-row {
    display: flex;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  .field {
    flex: 1;
    min-width: 150px;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--fs-xs);
  }
  .field-narrow {
    flex: 0 1 120px;
    min-width: 0;
  }
  .field-label {
    color: var(--color-text-muted);
  }
  .field input,
  .field textarea {
    font-size: var(--fs-sm);
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    border: var(--border-width) solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-text);
    width: 100%;
    box-sizing: border-box;
    transition: border-color var(--duration-fast) var(--ease-out), box-shadow var(--duration-fast) var(--ease-out);
  }
  .field textarea {
    resize: vertical;
    font-family: monospace;
  }
  .field input::placeholder,
  .field textarea::placeholder {
    color: var(--color-text-muted);
    opacity: 0.7;
  }
  .field input:focus,
  .field textarea:focus {
    outline: none;
    border-color: var(--color-primary);
    box-shadow: 0 0 0 3px color-mix(in oklch, var(--color-primary) 15%, transparent);
  }
  .field-toggle {
    flex: 0 0 auto;
    display: flex;
    flex-direction: row;
    align-items: center;
    white-space: nowrap;
    align-self: flex-end;
    padding-bottom: 4px;
  }

  .editor-item-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .editor-item-hint {
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }

  .modal-footer {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    gap: var(--space-2);
    padding-top: var(--space-3);
    border-top: var(--border-width) solid var(--color-border);
  }
  .hint {
    flex: 1;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }

  .btn {
    font-size: var(--fs-sm);
    padding: 5px var(--space-3);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text);
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out), border-color var(--duration-fast) var(--ease-out);
  }
  .btn:hover {
    background: var(--color-hover);
  }
  .btn.primary {
    background: var(--color-primary);
    border-color: var(--color-primary);
    color: var(--color-on-primary);
  }
  .btn.primary:hover {
    background: var(--color-primary-dim);
    border-color: var(--color-primary-dim);
  }
  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>

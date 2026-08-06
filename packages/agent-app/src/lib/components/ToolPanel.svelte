<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type {
    CommandToolConfig,
    HttpToolConfig,
    McpServerConfig,
    McpServerStatus,
    ToolConfigView,
    ToolInfo,
  } from "$lib/types";

  let tools = $state<ToolInfo[]>([]);
  let mcpServers = $state<McpServerStatus[]>([]);
  let loading = $state(true);
  let errorMsg = $state("");

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
  });

  async function refresh() {
    loading = true;
    errorMsg = "";
    try {
      const [toolList, serverList] = await Promise.all([
        invoke<ToolInfo[]>("list_tools"),
        invoke<McpServerStatus[]>("list_mcp_servers"),
      ]);
      tools = toolList;
      mcpServers = serverList;
    } catch (e) {
      errorMsg = `Load failed: ${e}`;
    } finally {
      loading = false;
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
      editorError = `Load config failed: ${e}`;
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
</script>

<div class="tools-panel">
  {#if errorMsg}
    <button class="error-banner" type="button" onclick={() => (errorMsg = "")}>
      <span class="error-text">{errorMsg}</span>
      <span class="error-dismiss" aria-hidden="true">×</span>
    </button>
  {/if}

  <div class="panel-toolbar">
    <span class="panel-title">工具</span>
    <button class="link-btn" type="button" onclick={openEditor}>编辑配置</button>
  </div>

  {#if loading}
    <p class="empty">加载中…</p>
  {:else}
    <section class="section">
      <div class="section-header">
        <span class="section-title">MCP Servers</span>
        <span class="section-count">{mcpServers.length}</span>
      </div>
      {#if mcpServers.length === 0}
        <p class="empty">暂无 MCP server，点右上角「编辑配置」添加</p>
      {:else}
        <ul class="server-list">
          {#each mcpServers as server (server.name)}
            <li class="server-item">
              <div class="server-main">
                <span class="server-name">{server.name}</span>
                <span class="server-meta">
                  <span class="transport">{server.transport}</span>
                  <span class="tool-count">{server.tool_count} tools</span>
                </span>
              </div>
              <span
                class="status"
                class:connected={server.status === "connected"}
                class:failed={server.status === "failed"}
                class:disabled={server.status === "disabled"}
              >
                <span class="status-dot" aria-hidden="true"></span>
                {server.status}
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
        <span class="section-title">工具</span>
        <span class="section-count">{tools.length}</span>
      </div>
      {#if tools.length === 0}
        <p class="empty">暂无可用工具</p>
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
  <div class="modal-overlay" role="presentation">
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-label="工具配置编辑"
      tabindex="-1"
    >
      <div class="modal-header">
        <span class="modal-title">工具配置</span>
        <button class="icon-btn" type="button" onclick={closeEditor} disabled={saving} aria-label="关闭">×</button>
      </div>

      {#if editorError}
        <button class="error-banner" type="button" onclick={() => (editorError = "")}>
          <span class="error-text">{editorError}</span>
          <span class="error-dismiss" aria-hidden="true">×</span>
        </button>
      {/if}

      {#if editorLoading}
        <p class="empty">加载配置中…</p>
      {:else}
        <div class="modal-body">
          <!-- MCP Servers -->
          <div class="editor-group">
            <div class="editor-group-header">
              <span>MCP Servers</span>
              <button class="link-btn" type="button" onclick={() => draft.mcp_servers.push(newMcpServer())}>+ 添加</button>
            </div>
            {#if draft.mcp_servers.length === 0}
              <p class="empty">暂无 MCP server</p>
            {:else}
              {#each draft.mcp_servers as server, i (i)}
                <div class="editor-item">
                  <div class="field-row">
                    <label class="field">
                      <span class="field-label">name</span>
                      <input type="text" bind:value={server.name} placeholder="filesystem" />
                    </label>
                    <label class="field">
                      <span class="field-label">transport</span>
                      <select bind:value={server.transport}>
                        <option value="stdio">stdio</option>
                        <option value="http">http</option>
                      </select>
                    </label>
                    <label class="field field-toggle">
                      <input type="checkbox" bind:checked={server.disabled} />
                      <span>disabled</span>
                    </label>
                  </div>
                  {#if server.transport === "stdio"}
                    <label class="field">
                      <span class="field-label">command</span>
                      <input type="text" bind:value={server.command} placeholder="npx" />
                    </label>
                    <label class="field">
                      <span class="field-label">args（逗号分隔）</span>
                      <input
                        type="text"
                        value={argsText(server.args)}
                        onchange={(e) => updateArgs(server, e.currentTarget.value)}
                        placeholder="-y, @modelcontextprotocol/server-filesystem, /tmp"
                      />
                    </label>
                  {:else}
                    <label class="field">
                      <span class="field-label">url</span>
                      <input type="text" bind:value={server.url} placeholder="http://127.0.0.1:8000/mcp" />
                    </label>
                  {/if}
                  <div class="editor-item-footer">
                    <span class="editor-item-hint">stdio 需 command；http 需 url</span>
                    <button class="link-btn danger" type="button" onclick={() => draft.mcp_servers.splice(i, 1)}>删除</button>
                  </div>
                </div>
              {/each}
            {/if}
          </div>

          <!-- HTTP Tools -->
          <div class="editor-group">
            <div class="editor-group-header">
              <span>HTTP Tools</span>
              <button class="link-btn" type="button" onclick={() => draft.http_tools.push(newHttpTool())}>+ 添加</button>
            </div>
            {#if draft.http_tools.length === 0}
              <p class="empty">暂无 HTTP tool</p>
            {:else}
              {#each draft.http_tools as tool, i (i)}
                <div class="editor-item">
                  <div class="field-row">
                    <label class="field">
                      <span class="field-label">name</span>
                      <input type="text" bind:value={tool.name} placeholder="lookup_wiki" />
                    </label>
                    <label class="field">
                      <span class="field-label">method</span>
                      <select bind:value={tool.method}>
                        <option value="GET">GET</option>
                        <option value="POST">POST</option>
                        <option value="PUT">PUT</option>
                        <option value="DELETE">DELETE</option>
                      </select>
                    </label>
                    <label class="field field-narrow">
                      <span class="field-label">timeout_ms</span>
                      <input type="number" bind:value={tool.timeout_ms} placeholder="可选" />
                    </label>
                  </div>
                  <label class="field">
                    <span class="field-label">desc</span>
                    <input type="text" bind:value={tool.desc} placeholder="工具描述" />
                  </label>
                  <label class="field">
                    <span class="field-label">url</span>
                    <input type="text" bind:value={tool.url} placeholder={"https://api.example.com/wiki?q={query}"} />
                  </label>
                  <div class="editor-item-footer">
                    <span class="editor-item-hint">端点固定，{'{query}'} 由模型填充</span>
                    <button class="link-btn danger" type="button" onclick={() => draft.http_tools.splice(i, 1)}>删除</button>
                  </div>
                </div>
              {/each}
            {/if}
          </div>

          <!-- Command Tools -->
          <div class="editor-group">
            <div class="editor-group-header">
              <span>Command Tools</span>
              <button class="link-btn" type="button" onclick={() => draft.command_tools.push(newCommandTool())}>+ 添加</button>
            </div>
            {#if draft.command_tools.length === 0}
              <p class="empty">暂无 command tool</p>
            {:else}
              {#each draft.command_tools as tool, i (i)}
                <div class="editor-item">
                  <div class="field-row">
                    <label class="field">
                      <span class="field-label">name</span>
                      <input type="text" bind:value={tool.name} placeholder="git_status" />
                    </label>
                    <label class="field field-narrow">
                      <span class="field-label">timeout_ms</span>
                      <input type="number" bind:value={tool.timeout_ms} placeholder="可选" />
                    </label>
                  </div>
                  <label class="field">
                    <span class="field-label">desc</span>
                    <input type="text" bind:value={tool.desc} placeholder="工具描述" />
                  </label>
                  <label class="field">
                    <span class="field-label">template（命令模板）</span>
                    <textarea bind:value={tool.template} rows="2" placeholder="git status --porcelain"></textarea>
                  </label>
                  <div class="editor-item-footer">
                    <span class="editor-item-hint">命令经过安全护栏：denylist / 超时 / 并发</span>
                    <button class="link-btn danger" type="button" onclick={() => draft.command_tools.splice(i, 1)}>删除</button>
                  </div>
                </div>
              {/each}
            {/if}
          </div>
        </div>

        <div class="modal-footer">
          <span class="hint">保存即生效：写回 JSON 并触发全量重装配</span>
          <button class="btn" type="button" onclick={closeEditor} disabled={saving}>取消</button>
          <button class="btn primary" type="button" onclick={saveConfig} disabled={saving || editorLoading}>
            {saving ? "保存中…" : "保存"}
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

  .link-btn {
    font-size: var(--fs-sm);
    font-weight: 500;
    color: var(--color-primary);
    background: transparent;
    border: none;
    padding: 2px 4px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .link-btn:hover {
    background: var(--color-hover);
  }
  .link-btn.danger {
    color: var(--color-error);
  }
  .link-btn:disabled {
    opacity: 0.4;
    cursor: default;
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
  .status.failed .status-dot { background: var(--color-error); }
  .status.failed { color: var(--color-error); }
  .status.disabled { opacity: 0.6; }

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
  .icon-btn {
    border: none;
    background: transparent;
    font-size: var(--fs-lg);
    line-height: 1;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
    padding: 2px 6px;
  }
  .icon-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .modal-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    overflow: auto;
    padding: 2px;
  }

  /* 编辑分组：单一容器 + 条目分隔线，避免嵌套卡片 */
  .editor-group {
    display: flex;
    flex-direction: column;
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
  .field select,
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
  .field select:focus,
  .field textarea:focus {
    outline: none;
    border-color: var(--color-primary);
    box-shadow: 0 0 0 3px color-mix(in oklch, var(--color-primary) 15%, transparent);
  }
  .field-toggle {
    flex: 0 1 auto;
    flex-direction: row;
    align-items: center;
    gap: var(--space-1);
    align-self: flex-end;
    padding-bottom: 4px;
  }
  .field-toggle input[type="checkbox"] {
    accent-color: var(--color-primary);
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

<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import Select from "./Select.svelte";
  import Toggle from "./Toggle.svelte";
  import Tooltip from "./Tooltip.svelte";
  import type {
    CommandToolConfig,
    HttpToolConfig,
    McpServerConfig,
    ToolConfigView,
  } from "$lib/types";
  import { useViewContext } from "$lib/layout/viewContext";

  const ctx = useViewContext();

  let loading = $state(true);
  let saving = $state(false);
  let error = $state("");
  let draft = $state<ToolConfigView>({
    mcp_servers: [],
    http_tools: [],
    command_tools: [],
  });

  onMount(load);

  async function load() {
    error = "";
    loading = true;
    try {
      const view = await invoke<ToolConfigView>("get_tool_config");
      draft = {
        mcp_servers: view.mcp_servers.map((s) => ({ ...s })),
        http_tools: view.http_tools.map((h) => ({ ...h })),
        command_tools: view.command_tools.map((c) => ({ ...c })),
      };
    } catch (e) {
      error = t("toolPanel.loadFailed", { error: `${e}` });
    } finally {
      loading = false;
    }
  }

  function closeEditor() {
    if (saving) return;
    ctx.commands.closeToolEditor();
  }

  async function saveConfig() {
    saving = true;
    error = "";
    try {
      const view = await invoke<ToolConfigView>("save_tool_config", {
        view: {
          mcp_servers: draft.mcp_servers,
          http_tools: draft.http_tools,
          command_tools: draft.command_tools,
        },
      });
      draft = view;
      await ctx.commands.closeToolEditor();
    } catch (e) {
      error = `${e}`;
    } finally {
      saving = false;
    }
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

<div class="tool-editor">
  <div class="editor-header">
    <span class="editor-title">{t("toolPanel.modalTitle")}</span>
  </div>

  {#if error}
    <button class="error-banner" type="button" onclick={() => (error = "")}>
      <span class="error-text">{error}</span>
      <span class="error-dismiss" aria-hidden="true">×</span>
    </button>
  {/if}

  {#if loading}
    <p class="empty">{t("toolPanel.loadingConfig")}</p>
  {:else}
    <div class="editor-body">
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

    <div class="editor-footer">
      <span class="hint">{t("toolPanel.saveHint")}</span>
      <button class="btn" type="button" onclick={closeEditor} disabled={saving}>{t("toolPanel.cancel")}</button>
      <button class="btn primary" type="button" onclick={saveConfig} disabled={saving}>
        {saving ? t("toolPanel.saving") : t("toolPanel.save")}
      </button>
    </div>
  {/if}
</div>

<style>
  .tool-editor {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    overflow: hidden;
  }

  .editor-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }
  .editor-title {
    font-size: var(--fs-lg);
    font-weight: 600;
    color: var(--color-text);
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

  .editor-body {
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

  .editor-footer {
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

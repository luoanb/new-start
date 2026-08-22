<script lang="ts">
  // SearchPanel：sidebar「搜索」视图（语义搜索，VSCode 全局搜索语义）。
  // - 输入关键词 → dataStore.semanticSearch（懒索引 + FTS5 块级检索，首次调用建索引）
  // - 结果按相关度排序（bm25 + 块类型加权），点击结果打开文件编辑器定位到块区间
  import { t } from "$lib/i18n";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import { layoutStore } from "$lib/layout/LayoutStore.svelte";
  import { fileEditorStore, fileKey } from "$lib/stores/fileEditorStore.svelte";
  import type { SearchBlock } from "$lib/types";

  // ── 派生：active 工作区（搜索与打开均以其为作用域）──
  let wsView = $derived(dataStore.state.workspaces);
  let workspaces = $derived(wsView?.workspaces ?? []);
  let activeWs = $derived(workspaces.find((w) => w.id === wsView?.active_id) ?? null);

  // ── State ──
  let query = $state("");
  let results = $state<SearchBlock[]>([]);
  let searching = $state(false);
  let searched = $state(false);
  let errorMsg = $state("");
  let meta = $state<{ blocks: number; ms: number } | null>(null);

  async function runSearch() {
    const q = query.trim();
    if (!q || searching) return;
    searching = true;
    searched = false;
    errorMsg = "";
    results = [];
    meta = null;
    try {
      const res = await dataStore.semanticSearch(q, 50);
      results = res.results;
      meta = { blocks: res.indexed_blocks, ms: res.index_duration_ms };
      searched = true;
    } catch (e) {
      errorMsg = t("searchPanel.loadFailed", { error: formatInvokeError(e) });
    } finally {
      searching = false;
    }
  }

  function onQueryKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") void runSearch();
  }

  /** 打开结果文件（相对 active 工作区路径，实例 key = `${wsId}:${relPath}`）。 */
  function openResult(r: SearchBlock) {
    if (!activeWs) return;
    const key = fileKey(activeWs.id, r.path);
    fileEditorStore.open(key, activeWs.id, r.path, null);
    layoutStore.insertPanel("file-editor", undefined, key);
  }

  /** 块类型徽章色调：容器类型（impl/trait/interface）与函数/类型区分。 */
  const TYPE_TONES: Record<string, string> = {
    Impl: "accent",
    Trait: "accent",
    Interface: "accent",
    Function: "primary",
    Struct: "primary",
    Class: "primary",
    Enum: "primary",
    File: "muted",
  };
  function toneFor(type: string): string {
    return TYPE_TONES[type] ?? "muted";
  }
</script>

<div class="search-panel">
  <div class="search-bar">
    <input
      class="query-input"
      bind:value={query}
      placeholder={t("searchPanel.placeholder")}
      aria-label={t("searchPanel.placeholder")}
      onkeydown={onQueryKeydown}
    />
    <button
      class="btn btn-sm btn-primary"
      onclick={() => void runSearch()}
      disabled={searching}
    >{t("searchPanel.search")}</button>
  </div>

  {#if searching}
    <p class="hint">{t("searchPanel.indexing")}</p>
  {:else if !activeWs}
    <p class="hint-empty">{t("searchPanel.noWorkspace")}</p>
  {:else if errorMsg}
    <p class="error-bar">{errorMsg}</p>
  {:else if !searched}
    <p class="hint">{t("searchPanel.noQuery")}</p>
  {:else if results.length === 0}
    <p class="hint-empty">{t("searchPanel.empty")}</p>
  {:else}
    <div class="result-list">
      {#each results as r (r.path + ":" + r.start_line + "-" + r.end_line)}
        <button class="result-row" onclick={() => openResult(r)} title={t("searchPanel.openInEditor")}>
          <div class="result-head">
            <span class="result-path">{r.path}</span>
            <span class="result-lines">{r.start_line}–{r.end_line}</span>
          </div>
          <div class="result-meta">
            <span class="type-badge {toneFor(r.block_type)}">{r.block_type}</span>
            <span class="result-score">{r.score.toFixed(2)}</span>
          </div>
          <pre class="result-content">{r.content.trim()}</pre>
        </button>
      {/each}
    </div>
    {#if meta}
      <p class="meta-bar">
        {t("searchPanel.results", { n: results.length })} · {t("searchPanel.blocks", {
          n: meta.blocks,
          ms: meta.ms,
        })}
      </p>
    {/if}
  {/if}
</div>

<style>
  .search-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--color-surface);
    font-size: var(--fs-sm);
  }

  .search-bar {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-2);
    border-bottom: var(--border-width) solid var(--color-border);
    flex-shrink: 0;
  }
  .query-input {
    flex: 1;
    min-width: 0;
    padding: 4px 8px;
    font-size: var(--fs-sm);
    background: var(--color-elevated);
    color: var(--color-text);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    outline: none;
  }
  .query-input:focus {
    border-color: var(--color-primary);
  }

  .hint,
  .hint-empty {
    padding: var(--space-3) var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--fs-sm);
    text-align: center;
  }
  .error-bar {
    padding: var(--space-2) var(--space-2);
    color: var(--color-danger);
    font-size: var(--fs-xs);
    white-space: pre-wrap;
  }

  .result-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-1);
  }
  .result-row {
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 100%;
    padding: var(--space-1) var(--space-2);
    border: none;
    background: transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
  }
  .result-row:hover {
    background: var(--color-hover);
  }

  .result-head {
    display: flex;
    align-items: baseline;
    gap: var(--space-1);
    min-width: 0;
  }
  .result-path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text);
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
  }
  .result-lines {
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }

  .result-meta {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .type-badge {
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--color-text-muted);
  }
  .type-badge.primary {
    color: var(--color-primary);
  }
  .type-badge.accent {
    color: var(--color-accent, var(--color-primary));
  }
  .type-badge.muted {
    color: var(--color-text-muted);
  }
  .result-score {
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    font-variant-numeric: tabular-nums;
  }

  .result-content {
    margin: 0;
    max-height: 96px;
    overflow: hidden;
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    line-height: 1.5;
    color: var(--color-text-muted);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .meta-bar {
    flex-shrink: 0;
    padding: var(--space-1) var(--space-2);
    border-top: var(--border-width) solid var(--color-border);
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
  }
</style>

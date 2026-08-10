<script lang="ts">
  import { t } from "$lib/i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { useViewContext } from "$lib/layout/viewContext";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import type { Neuron, NeuronPage } from "$lib/types";
  import { systemTypeColor } from "$lib/features/neuron/systemTypeColor";

  const ctx = useViewContext();
  const data = ctx.stores.data;

  const PAGE_SIZE = 20;

  // ── 列表状态 ──
  let items = $state<Neuron[]>([]);
  let total = $state(0);
  let hasMore = $state(false);
  let page = $state(0);
  let loading = $state(true);
  let loadingMore = $state(false);
  let errorMsg = $state("");

  // ── 工具栏 ──
  let search = $state("");
  let kind = $state<"all" | "system" | "normal">("all");
  let multi = $derived(data.state.neuronSelectionMode === "multi");
  let selection = $derived(data.state.neuronSelection);

  // ── 共享状态 ──
  let listEl = $state<HTMLElement | null>(null);

  async function reload() {
    page = 0;
    loading = true;
    errorMsg = "";
    try {
      const res = await invoke<NeuronPage>("list_neurons_page", {
        page: 0,
        pageSize: PAGE_SIZE,
        search: search || null,
        kind,
      });
      items = res.items;
      total = res.total;
      hasMore = res.has_more;
    } catch (e) {
      errorMsg = formatInvokeError(e);
    } finally {
      loading = false;
    }
  }

  async function loadMore() {
    if (loadingMore || !hasMore || loading) return;
    loadingMore = true;
    try {
      const next = page + 1;
      const res = await invoke<NeuronPage>("list_neurons_page", {
        page: next,
        pageSize: PAGE_SIZE,
        search: search || null,
        kind,
      });
      items = [...items, ...res.items];
      total = res.total;
      hasMore = res.has_more;
      page = next;
    } catch (e) {
      errorMsg = formatInvokeError(e);
    } finally {
      loadingMore = false;
    }
  }

  // 搜索防抖（200ms）
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    search;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => void reload(), 200);
  });

  // 类型筛选变更重置
  $effect(() => {
    kind;
    void reload();
  });

  // 后端写入（创建/编辑/绑定行为）广播 Neurons：重载第 0 页。
  let lastNeuronsVersion = 0;
  $effect(() => {
    const version = data.state.neuronsVersion;
    if (version !== lastNeuronsVersion) {
      lastNeuronsVersion = version;
      void reload();
    }
  });

  // 滚动到底加载更多
  function handleScroll() {
    if (!listEl) return;
    if (listEl.scrollTop + listEl.clientHeight >= listEl.scrollHeight - 24) {
      void loadMore();
    }
  }

  function handleRowClick(n: Neuron) {
    if (data.state.neuronSelectionMode === "multi") {
      data.toggleNeuronSelection(n.id);
    } else {
      data.setNeuronSelection([n.id]);
    }
  }

  function handleEdit(n: Neuron) {
    data.requestEditNeuron(n.id);
  }

  function handleLaunch(n: Neuron) {
    void data.requestLaunchNeuron(n.id);
  }
</script>

<div class="neurons-list-panel">
  <div class="toolbar">
    <input
      class="search"
      placeholder={t("neuronListPanel.search")}
      bind:value={search}
    />
    <select class="kind-select" bind:value={kind}>
      <option value="all">{t("neuronListPanel.kindAll")}</option>
      <option value="system">{t("neuronListPanel.kindSystem")}</option>
      <option value="normal">{t("neuronListPanel.kindNormal")}</option>
    </select>
    <label class="multi-toggle">
      <input
        type="checkbox"
        checked={multi}
        onchange={() => {
          data.state.neuronSelectionMode = multi ? "single" : "multi";
          if (multi) data.setNeuronSelection([]);
        }}
      />
      {t("neuronListPanel.multiSelect")}
    </label>
    <button class="btn btn-primary" onclick={() => data.requestCreateNeuron()}>
      ＋ {t("neuronListPanel.create")}
    </button>
  </div>

  {#if errorMsg}
    <p class="error">{errorMsg}</p>
  {/if}

  {#if loading}
    <p class="empty">{t("neuronListPanel.loading")}</p>
  {:else if items.length === 0}
    <p class="empty">{t("neuronListPanel.empty")}</p>
  {:else}
    <div class="list" bind:this={listEl} onscroll={handleScroll}>
      {#each items as n (n.id)}
        <button
          class="item"
          class:selected={selection.includes(n.id)}
          onclick={() => handleRowClick(n)}
        >
          <div class="item-top">
            <span
              class="type-badge"
              class:normal={!n.system_type}
              style:background={systemTypeColor(n.system_type)}
            >
              {n.system_type ?? t("neuronListPanel.kindNormal")}
            </span>
            <span class="item-desc" title={n.desc}>{n.desc || n.id}</span>
          </div>
          <div class="item-meta">
            <span class="mono">{n.id}</span>
            <span class="weight">w{n.weight.toFixed(3)}</span>
          </div>
          <div class="item-actions">
            <span
              class="row-btn"
              role="button"
              tabindex="0"
              onclick={(e) => {
                e.stopPropagation();
                handleEdit(n);
              }}
            >
              {t("neuronListPanel.edit")}
            </span>
            {#if n.system_type}
              <span
                class="row-btn launch"
                role="button"
                tabindex="0"
                title={t("neuronListPanel.launchHint")}
                onclick={(e) => {
                  e.stopPropagation();
                  handleLaunch(n);
                }}
              >
                {t("neuronListPanel.launch")}
              </span>
            {/if}
          </div>
        </button>
      {/each}
      {#if hasMore}
        <button class="load-more" disabled={loadingMore} onclick={() => void loadMore()}>
          {loadingMore ? t("neuronListPanel.loading") : t("neuronListPanel.loadMore")}
        </button>
      {:else}
        <p class="no-more">{t("neuronListPanel.noMore")}</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .neurons-list-panel {
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-2);
    border-bottom: var(--border-width) solid var(--color-border);
    flex-wrap: wrap;
  }
  .search {
    flex: 1;
    min-width: 90px;
    background: var(--color-bg);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: 4px 8px;
    color: var(--color-text);
    font-size: var(--fs-xs);
  }
  .search:focus {
    outline: none;
    border-color: var(--color-primary);
  }
  .kind-select {
    background: var(--color-bg);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text);
    font-size: var(--fs-xs);
    padding: 4px 6px;
  }
  .multi-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    cursor: pointer;
    white-space: nowrap;
  }
  .multi-toggle input {
    accent-color: var(--color-primary);
  }
  .btn-primary {
    border: var(--border-width) solid var(--color-primary);
    background: var(--color-primary);
    color: var(--color-on-primary);
    border-radius: var(--radius-sm);
    padding: 3px 10px;
    font-size: var(--fs-xs);
    cursor: pointer;
    white-space: nowrap;
  }

  .error {
    font-size: var(--fs-xs);
    color: var(--color-danger, #e5484d);
    padding: var(--space-2);
  }
  .empty {
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--fs-sm);
    padding: var(--space-6) var(--space-2);
  }

  .list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-2);
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-2);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    border: var(--border-width) solid var(--color-border);
    color: var(--color-text);
    text-align: left;
    cursor: pointer;
    transition:
      border-color 0.15s ease,
      background 0.15s ease;
  }
  .item:hover {
    border-color: var(--color-primary);
  }
  .item.selected {
    border-color: var(--color-primary);
    background: color-mix(in srgb, var(--color-primary) 10%, var(--color-bg));
  }
  .item-top {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }
  .type-badge {
    flex-shrink: 0;
    font-size: 10px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    color: var(--color-on-primary, #fff);
    opacity: 0.92;
    max-width: 110px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .type-badge.normal {
    background: var(--color-text-muted);
    opacity: 0.65;
  }
  .item-desc {
    flex: 1;
    min-width: 0;
    font-size: var(--fs-sm);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-1);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .mono {
    font-family: var(--font-mono);
    font-size: 10.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item-actions {
    display: flex;
    gap: var(--space-1);
  }
  .row-btn {
    font-size: var(--fs-xs);
    color: var(--color-primary);
    cursor: pointer;
    user-select: none;
  }
  .row-btn:hover {
    text-decoration: underline;
  }
  .row-btn.launch {
    color: var(--color-system-assistant);
  }
  .load-more {
    border: var(--border-width) dashed var(--color-border);
    background: transparent;
    color: var(--color-text-muted);
    border-radius: var(--radius-md);
    padding: 6px;
    font-size: var(--fs-xs);
    cursor: pointer;
  }
  .load-more:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .no-more {
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    margin: var(--space-1) 0;
  }
</style>

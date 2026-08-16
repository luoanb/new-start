<script lang="ts">
  import { t } from "$lib/i18n";
  import { api } from "$lib/api";
  import { useViewContext } from "$lib/layout/viewContext";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import type { Neuron, NeuronPage } from "$lib/types";
  import { systemTypeColor } from "$lib/features/neuron/systemTypeColor";
  import Select from "./Select.svelte";

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
  const kindOptions = [
    { value: "all", label: t("neuronListPanel.kindAll") },
    { value: "system", label: t("neuronListPanel.kindSystem") },
    { value: "normal", label: t("neuronListPanel.kindNormal") },
  ];

  // ── 共享状态 ──
  let listEl = $state<HTMLElement | null>(null);

  async function reload() {
    page = 0;
    loading = true;
    errorMsg = "";
    try {
      const res = await api.invoke<NeuronPage>("list_neurons_page", {
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
      const res = await api.invoke<NeuronPage>("list_neurons_page", {
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
    // 列表项点击 → 确保主区画布打开（已打开则仅激活）
    ctx.stores.layout.insertPanel("neurons");
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
    <Select
      class="kind-select"
      value={kind}
      options={kindOptions}
      onchange={(v) => (kind = v as typeof kind)}
    />
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
    <button class="btn btn-sm btn-primary" onclick={() => data.requestCreateNeuron()}>
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
            {#if n.system_type}
              <svg
                class="item-icon"
                viewBox="0 0 24 24"
                width="14"
                height="14"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
                style:color={systemTypeColor(n.system_type)}
              >
                <circle cx="5" cy="6" r="2" />
                <circle cx="19" cy="7" r="2" />
                <circle cx="12" cy="18" r="2" />
                <line x1="6.5" y1="7" x2="11" y2="16" />
                <line x1="17.5" y1="8" x2="13" y2="16" />
              </svg>
            {:else}
              <svg
                class="item-icon"
                viewBox="0 0 24 24"
                width="14"
                height="14"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <circle cx="12" cy="12" r="4" />
              </svg>
            {/if}
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
              onkeydown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  e.stopPropagation();
                  handleEdit(n);
                }
              }}
            >
              <svg
                viewBox="0 0 24 24"
                width="12"
                height="12"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="M17 3a2.83 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
              </svg>
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
                onkeydown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    e.stopPropagation();
                    handleLaunch(n);
                  }
                }}
              >
                <svg
                  viewBox="0 0 24 24"
                  width="12"
                  height="12"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.8"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <polygon points="5 3 19 12 5 21 5 3" />
                </svg>
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
    padding: var(--space-1) var(--space-2);
    color: var(--color-text);
    font-size: var(--fs-xs);
  }
  .search:focus {
    outline: none;
    border-color: var(--color-primary);
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
  .item-icon {
    flex-shrink: 0;
    display: inline-flex;
    color: var(--color-text-muted);
    opacity: 0.85;
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
    gap: var(--space-2);
  }
  .row-btn {
    display: inline-flex;
    align-items: center;
    gap: 3px;
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

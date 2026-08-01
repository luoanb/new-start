<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { Connection, Neuron, NeuronSubgraph } from "$lib/types";
  import { t } from "$lib/i18n";
  import NeuronIndex from "./NeuronIndex.svelte";
  import NeuronNetworkGraph from "./NeuronNetworkGraph.svelte";
  import NeuronDetailDrawer from "./NeuronDetailDrawer.svelte";

  let neurons = $state<Neuron[]>([]);
  let linkCounts = $state<Record<string, number>>({});
  let loading = $state(true);
  let error = $state<string | null>(null);

  // Graph-first 状态
  let selectedId = $state<string | null>(null);
  let subgraph = $state<NeuronSubgraph>({ seed_id: "", neurons: [], connections: [] });

  // 抽屉
  let drawerNeuron = $state<Neuron | null>(null);
  let drawerConns = $state<Connection[]>([]);

  // 过滤 / 搜索
  let search = $state("");
  let activeTypes = $state<string[]>([]); // 空 = 全部
  let depth = $state(2);

  let allTypes = $derived(
    Array.from(new Set(neurons.map((n) => n.system_type || "uncategorized"))).sort()
  );

  let filteredNeurons = $derived(
    neurons.filter((n) => {
      const type = n.system_type || "uncategorized";
      if (activeTypes.length && !activeTypes.includes(type)) return false;
      if (search && !`${n.desc} ${n.id}`.toLowerCase().includes(search.toLowerCase()))
        return false;
      return true;
    })
  );

  // 可见节点 id 集合（过滤后）
  let visibleIds = $derived(new Set(filteredNeurons.map((n) => n.id)));

  // 构建全局图的 subgraph：在可见节点内取 top-N（按权重）+ 其邻居
  const TOP_N = 60;
  function buildSubgraph(): NeuronSubgraph {
    const visible = filteredNeurons;
    if (visible.length === 0) return { seed_id: "", neurons: [], connections: [] };

    // 按权重排序取 top-N 作为核心节点
    const sorted = [...visible].sort((a, b) => b.weight - a.weight);
    const core = sorted.slice(0, TOP_N);
    const coreIds = new Set(core.map((n) => n.id));

    // 收集核心节点与其邻居（邻居也必须在可见集合内）
    const nodeIds = new Set<string>(coreIds);
    for (const c of allConnections) {
      if (coreIds.has(c.source) && visibleIds.has(c.target)) nodeIds.add(c.target);
      if (coreIds.has(c.target) && visibleIds.has(c.source)) nodeIds.add(c.source);
    }

    // 深度剪枝：仅保留从核心出发 depth 跳内的连接
    const chosen = pruneByDepth(coreIds, depth);
    const finalIds = new Set<string>([...nodeIds, ...chosen]);

    const subNeurons = visible.filter((n) => finalIds.has(n.id));
    const subConns = allConnections.filter(
      (c) => finalIds.has(c.source) && finalIds.has(c.target)
    );
    return { seed_id: core[0]?.id ?? "", neurons: subNeurons, connections: subConns };
  }

  // 从核心节点按 BFS 限制展开深度
  function pruneByDepth(coreIds: Set<string>, maxDepth: number): Set<string> {
    const adj = new Map<string, string[]>();
    for (const c of allConnections) {
      if (!adj.has(c.source)) adj.set(c.source, []);
      if (!adj.has(c.target)) adj.set(c.target, []);
      adj.get(c.source)!.push(c.target);
      adj.get(c.target)!.push(c.source);
    }
    const visited = new Set(coreIds);
    let frontier = [...coreIds];
    for (let d = 0; d < maxDepth; d++) {
      const next: string[] = [];
      for (const id of frontier) {
        for (const nb of adj.get(id) ?? []) {
          if (!visited.has(nb) && visibleIds.has(nb)) {
            visited.add(nb);
            next.push(nb);
          }
        }
      }
      frontier = next;
    }
    return visited;
  }

  let allConnections = $state<Connection[]>([]);

  // 过滤变化时重建图
  $effect(() => {
    // 依赖：filteredNeurons / depth / visibleIds
    filteredNeurons;
    depth;
    subgraph = buildSubgraph();
  });

  async function load() {
    loading = true;
    error = null;
    try {
      const list = (await invoke("list_neurons")) as Neuron[];
      neurons = list.sort((a, b) => b.weight - a.weight);

      // 拉取连接数（用于索引侧栏徽章）
      const counts: Record<string, number> = {};
      const conns: Connection[] = [];
      await Promise.all(
        neurons.map(async (n) => {
          try {
            const cs = (await invoke("get_connections", { id: n.id })) as Connection[];
            counts[n.id] = cs.length;
            conns.push(...cs);
          } catch {
            counts[n.id] = 0;
          }
        })
      );
      linkCounts = counts;
      allConnections = conns;
      subgraph = buildSubgraph();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function selectNeuron(id: string) {
    selectedId = id;
    openDrawer(id);
  }

  async function openDrawer(id: string) {
    const n = neurons.find((x) => x.id === id);
    if (!n) return;
    drawerNeuron = n;
    drawerConns = allConnections.filter((c) => c.source === id || c.target === id);
  }

  function closeDrawer() {
    drawerNeuron = null;
    selectedId = null;
  }

  function toggleType(type: string) {
    if (activeTypes.includes(type)) {
      activeTypes = activeTypes.filter((x) => x !== type);
    } else {
      activeTypes = [...activeTypes, type];
    }
  }

  function clearFilters() {
    search = "";
    activeTypes = [];
    depth = 2;
  }

  onMount(() => {
    load();
  });
</script>

<div class="neuron-manager">
  <div class="toolbar">
    <input
      class="search"
      placeholder={t("neuronPanel.search")}
      bind:value={search}
    />
    <div class="filters">
      <button
        class="chip"
        class:active={activeTypes.length === 0}
        on:click={() => (activeTypes = [])}
      >
        {t("neuronPanel.filterAll")}
      </button>
      {#each allTypes as type (type)}
        <button
          class="chip"
          class:active={activeTypes.includes(type)}
          style:--chip-color={`var(--color-system-${type}, var(--color-system-default))`}
          on:click={() => toggleType(type)}
        >
          {type}
        </button>
      {/each}
    </div>
    <div class="depth">
      <span class="depth-label">{t("neuronPanel.depthLabel")}</span>
      <input type="range" min="1" max="5" step="1" bind:value={depth} />
      <span class="depth-val">{depth}</span>
    </div>
    {#if search || activeTypes.length}
      <button class="clear" on:click={clearFilters}>✕</button>
    {/if}
  </div>

  <div class="body">
    <aside class="sidebar">
      <NeuronIndex
        neurons={filteredNeurons}
        {selectedId}
        {linkCounts}
        onSelect={selectNeuron}
      />
    </aside>

    <main class="canvas">
      {#if loading}
        <div class="state">{t("neuronPanel.loading")}</div>
      {:else if error}
        <div class="state error">{error}</div>
      {:else if subgraph.neurons.length === 0}
        <div class="state empty">
          <div class="empty-title">{t("neuronPanel.emptyTitle")}</div>
          <div class="empty-hint">{t("neuronPanel.emptyHint")}</div>
        </div>
      {:else}
        <div class="graph-host" role="presentation">
          <NeuronNetworkGraph
            {subgraph}
            {selectedId}
            onJumpTo={selectNeuron}
          />
        </div>
      {/if}
    </main>

    <NeuronDetailDrawer
      neuron={drawerNeuron}
      connections={drawerConns}
      onClose={closeDrawer}
      onJumpTo={selectNeuron}
    />
  </div>
</div>

<style>
  .neuron-manager {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-surface);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--color-border);
    flex-wrap: wrap;
  }

  .search {
    flex: 1;
    min-width: 140px;
    max-width: 260px;
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 6px 10px;
    color: var(--color-text);
    font-size: 12.5px;
  }
  .search:focus {
    outline: none;
    border-color: var(--color-primary);
  }

  .filters {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .chip {
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text-muted);
    border-radius: 999px;
    padding: 3px 10px;
    font-size: 11px;
    cursor: pointer;
    transition:
      background 0.15s ease,
      color 0.15s ease,
      border-color 0.15s ease;
  }
  .chip:hover {
    color: var(--color-text);
  }
  .chip.active {
    color: var(--color-on-primary);
    background: var(--chip-color, var(--color-primary));
    border-color: var(--chip-color, var(--color-primary));
  }

  .depth {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .depth-label {
    font-size: 11px;
    color: var(--color-text-muted);
  }
  .depth-val {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text);
    width: 10px;
    text-align: center;
  }

  .clear {
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 13px;
  }
  .clear:hover {
    color: var(--color-text);
  }

  .body {
    flex: 1;
    display: grid;
    grid-template-columns: 248px 1fr;
    min-height: 0;
    position: relative;
  }

  .sidebar {
    border-right: 1px solid var(--color-border);
    min-height: 0;
    overflow: hidden;
  }

  .canvas {
    position: relative;
    min-height: 0;
    overflow: hidden;
  }

  .graph-host {
    position: absolute;
    inset: 0;
  }

  .state {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 13px;
    text-align: center;
    padding: 24px;
  }
  .state.error {
    color: var(--color-error);
  }
  .empty-title {
    font-size: 15px;
    color: var(--color-text);
    font-weight: 600;
  }
  .empty-hint {
    font-size: 12px;
    max-width: 280px;
    line-height: 1.5;
  }

  @media (max-width: 800px) {
    .body {
      grid-template-columns: 1fr;
    }
    .sidebar {
      display: none;
    }
  }
</style>

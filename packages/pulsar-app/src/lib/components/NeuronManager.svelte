<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { Connection, Neuron, NeuronSubgraph } from "$lib/types";
  import { t } from "$lib/i18n";
  import NeuronNetworkGraph from "./NeuronNetworkGraph.svelte";
  import NeuronDetailDrawer from "./NeuronDetailDrawer.svelte";
  import Select from "./Select.svelte";
  import {
    readLayoutPref,
    writeLayoutPref,
    layoutOptions,
    type LayoutId,
  } from "$lib/features/neuron/networkLayout";
  import { errorMessage } from "$lib/errorMessage";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import NeuronList from "./NeuronList.svelte";
  import NeuronDetail from "./NeuronDetail.svelte";
  import NeuronNetwork from "./NeuronNetwork.svelte";

  let neurons = $state<Neuron[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  // Graph-first 状态
  let selectedId = $state<string | null>(null);
  let subgraph = $state<NeuronSubgraph>({ seed_id: "", neurons: [], connections: [] });

  // 画布 seed：由列表共享选择驱动（dataStore.neuronSelection[0]），画布内切换写回共享状态
  let canvasSeed = $state<string | null>(null);
  // 布局算法（力导向/分层），选择持久化到 localStorage
  let layoutId = $state<LayoutId>(readLayoutPref());

  // 抽屉
  let drawerNeuron = $state<Neuron | null>(null);
  let drawerConns = $state<Connection[]>([]);

  // 过滤 / 搜索（v9：搜索与核心筛选上移到列表视图，画布仅保留深度/布局/连线）
  let depth = $state(1);

  // 连线类型（力导向布局默认 floating：自动吸附卡片最近边缘点）
  type EdgeType = "bezier" | "smoothstep" | "step" | "straight" | "floating";
  let edgeType = $state<EdgeType>("floating");

  // 可见节点 id 集合（v9：无搜索过滤，全量可见；pruneByDepth 仍以它为界）
  let visibleIds = $derived(new Set(neurons.map((n) => n.id)));

  // 构建全局图的 subgraph：以画布 seed 为起点，按 depth 展开（seed 是唯一展开根）
  function buildSubgraph(seedId: string): NeuronSubgraph {
    if (neurons.length === 0)
      return { seed_id: "", neurons: [], connections: [] };

    const coreIds = new Set([seedId]);

    // 从 seed 节点 BFS 展开 depth 跳（seed + 跳内节点）
    const finalIds = pruneByDepth(coreIds, depth);

    const subNeurons = neurons.filter((n) => finalIds.has(n.id));
    const subConns = allConnections.filter(
      (c) => finalIds.has(c.source) && finalIds.has(c.target),
    );
    return { seed_id: seedId, neurons: subNeurons, connections: subConns };
  }

  // 当前 subgraph 内权重 min/max：节点尺寸/配色归一化（布局与渲染同源）
  const { minW, maxW } = $derived.by(() => {
    const ws = subgraph.neurons.map((n) => n.weight);
    return ws.length
      ? { minW: Math.min(...ws), maxW: Math.max(...ws) }
      : { minW: 0, maxW: 1 };
  });

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

  // 列表共享选择 → 画布 seed 单向同步（画布内切换 seed 写回共享状态）
  $effect(() => {
    const selection = dataStore.state.neuronSelection;
    canvasSeed = selection.length > 0 ? selection[0] : null;
  });

  // 过滤 / 深度 / 画布 seed 变化时重建图
  $effect(() => {
    // 依赖：neurons / depth / canvasSeed / visibleIds
    neurons;
    depth;
    canvasSeed;
    if (!canvasSeed) {
      subgraph = { seed_id: "", neurons: [], connections: [] };
      return;
    }
    subgraph = buildSubgraph(canvasSeed);
  });

  async function load() {
    loading = true;
    error = null;
    try {
      const list = (await invoke("list_neurons")) as Neuron[];
      neurons = list.sort((a, b) => b.weight - a.weight);

      // 拉取全部连接（用于图）
      const conns: Connection[] = [];
      await Promise.all(
        neurons.map(async (n) => {
          try {
            const cs = (await invoke("get_connections", { id: n.id })) as Connection[];
            conns.push(...cs);
          } catch {
            // 忽略单节点拉取失败
          }
        })
      );

      // 方向敏感去重：保留 A→B 与 B→A 共存，仅丢弃 source+target 完全相同的重复连接
      const seen = new Set<string>();
      const deduped: Connection[] = [];
      for (const c of conns) {
        const key = c.source + "->" + c.target;
        if (seen.has(key)) continue;
        seen.add(key);
        deduped.push(c);
      }
      allConnections = deduped;

      if (canvasSeed) subgraph = buildSubgraph(canvasSeed);
    } catch (e) {
      console.error(`Failed to load neurons: ${errorMessage(e)}`);
    } finally {
      loading = false;
    }
  }

  function selectNeuron(id: string) {
    selectedId = id;
    openDrawer(id);
  }

  // 抽屉保存 / 权重调整后：从后端拉取该神经元最新数据并同步到列表与图（避免用过期快照覆盖已保存值）
  async function refreshDrawerAndGraph() {
    if (!drawerNeuron) return;
    const id = drawerNeuron.id;
    try {
      const n = (await invoke("get_neuron", { id })) as Neuron;
      drawerNeuron = n;
      neurons = neurons.map((x) => (x.id === id ? n : x));
      const cs = (await invoke("get_connections", { id })) as Connection[];
      drawerConns = cs;
      // 更新全局连接快照中该节点相关的边
      allConnections = [
        ...allConnections.filter((c) => c.source !== id && c.target !== id),
        ...cs,
      ];
      if (canvasSeed) subgraph = buildSubgraph(canvasSeed);
    } catch {
      // 忽略刷新失败，抽屉已显示最新返回值
    }
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

  // 创建神经元弹窗
  let showCreate = $state(false);
  let createMode = $state<"orphan" | "downstream">("orphan");
  let createDesc = $state("");
  let createContent = $state("");
  let createSource = $state<string>("");
  let creating = $state(false);
  let createError = $state<string | null>(null);
  let createToolIds = $state<string[]>([]);
  let availableTools = $state<{ name: string; description: string }[]>([]);

  async function loadAvailableTools() {
    try {
      availableTools = (await invoke("list_skills")) as {
        name: string;
        description: string;
      }[];
    } catch {
      availableTools = [];
    }
  }

  // 顶栏：直接进入孤立模式
  function openCreateOrphan() {
    createMode = "orphan";
    createSource = "";
    createDesc = "";
    createContent = "";
    createToolIds = [];
    createError = null;
    showCreate = true;
  }

  // 节点抽屉：以当前神经元为上游，进入下游模式
  function requestCreateDownstream(sourceId: string) {
    createMode = "downstream";
    createSource = sourceId;
    createDesc = "";
    createContent = "";
    createToolIds = [];
    createError = null;
    showCreate = true;
  }

  function toggleCreateTool(name: string) {
    createToolIds = createToolIds.includes(name)
      ? createToolIds.filter((x) => x !== name)
      : [...createToolIds, name];
  }

  async function submitCreate() {
    const desc = createDesc.trim();
    if (!desc) {
      createError = t("neuronPanel.createDescRequired");
      return;
    }
    if (createMode === "downstream" && !createSource) {
      createError = t("neuronPanel.createSourceRequired");
      return;
    }
    creating = true;
    createError = null;
    try {
      const created = (await invoke("create_neuron_plain", {
        desc,
        content: createContent,
        linkTo: createMode === "downstream" ? createSource : null,
        toolIds: createToolIds,
      })) as Neuron;
      showCreate = false;
      createDesc = "";
      createContent = "";
      createSource = "";
      createToolIds = [];
      createMode = "orphan";
      await load();
      selectNeuron(created.id);
    } catch (e) {
      createError = formatInvokeError(e);
    } finally {
      creating = false;
    }
  }

  function cancelCreate() {
    showCreate = false;
    createError = null;
    createDesc = "";
    createContent = "";
    createSource = "";
    createToolIds = [];
    createMode = "orphan";
  }

  onMount(() => {
    load();
    loadAvailableTools();
  });

  // 列表「编辑」→ 打开抽屉（消费后置 null）。
  // 依赖 neurons：面板刚挂载且数据未加载完成时先不消费，等 load() 完成后重跑。
  let lastEditRequestId = $state<string | null>(null);
  $effect(() => {
    const reqId = dataStore.state.neuronEditRequestId;
    if (!reqId || reqId === lastEditRequestId) return;
    if (!neurons.some((n) => n.id === reqId)) return;
    lastEditRequestId = reqId;
    dataStore.state.neuronEditRequestId = null;
    openDrawer(reqId);
  });

  // 列表「＋ 创建」→ 打开创建弹窗（计数触发，消费）。
  let lastCreateRequest = $state(0);
  $effect(() => {
    const req = dataStore.state.neuronCreateRequest;
    if (req !== lastCreateRequest) {
      lastCreateRequest = req;
      openCreateOrphan();
    }
  });

  // 画布内切换 seed：单选替换 / 多选 append（去重），写回列表共享状态
  function handleSetSeed(id: string) {
    if (dataStore.state.neuronSelectionMode === "multi") {
      if (!dataStore.state.neuronSelection.includes(id)) {
        dataStore.setNeuronSelection([...dataStore.state.neuronSelection, id]);
      }
    } else {
      dataStore.setNeuronSelection([id]);
    }
  }

  // 人工评价/权重变化（StateChange::Neurons）后自动刷新列表与网络。
  let lastNeuronsVersion = 0;
  $effect(() => {
    const version = dataStore.state.neuronsVersion;
    if (version !== lastNeuronsVersion) {
      lastNeuronsVersion = version;
      void load();
    }
  });
</script>

<div class="neuron-manager">
  <div class="toolbar">
    <button class="create-btn" on:click={openCreateOrphan}>
      ＋ {t("neuronPanel.create")}
    </button>
    <div class="depth">
      <span class="depth-label">{t("neuronPanel.depthLabel")}</span>
      <input type="range" min="1" max="5" step="1" bind:value={depth} />
      <span class="depth-val">{depth}</span>
    </div>
    <div class="layout-type">
      <span class="depth-label">{t("neuronPanel.layoutLabel")}</span>
      <Select
        bind:value={layoutId}
        options={layoutOptions.map((a) => ({ value: a.id, label: t(a.labelKey) }))}
        onchange={(v) => writeLayoutPref(v as LayoutId)}
      />
    </div>
    <div class="edge-type">
      <span class="depth-label">{t("neuronPanel.edgeTypeLabel")}</span>
      <Select
        bind:value={edgeType}
        options={[
          { value: "floating", label: t("neuronPanel.edgeFloating") },
          { value: "bezier", label: t("neuronPanel.edgeBezier") },
          { value: "smoothstep", label: t("neuronPanel.edgeSmoothstep") },
          { value: "step", label: t("neuronPanel.edgeStep") },
          { value: "straight", label: t("neuronPanel.edgeStraight") },
        ]}
      />
    </div>
  </div>

  <div class="body">
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
            {edgeType}
            {layoutId}
            {minW}
            {maxW}
            onJumpTo={selectNeuron}
            onSetSeed={handleSetSeed}
          />
        </div>
      {/if}
    </main>

    <NeuronDetailDrawer
      neuron={drawerNeuron}
      connections={drawerConns}
      onClose={closeDrawer}
      onJumpTo={selectNeuron}
      onChanged={() => refreshDrawerAndGraph()}
      onRequestCreateDownstream={requestCreateDownstream}
    />
  </div>

  {#if showCreate}
    <div class="modal-mask" role="presentation" on:click={cancelCreate}>
      <div class="modal" role="dialog" aria-modal="true" on:click|stopPropagation>
        <div class="modal-title">{t("neuronPanel.createTitle")}</div>

        <div class="modal-row modes">
          <button
            class="mode-btn"
            class:active={createMode === "orphan"}
            on:click={() => (createMode = "orphan")}
          >
            {t("neuronPanel.createOrphan")}
          </button>
          <button
            class="mode-btn"
            class:active={createMode === "downstream"}
            on:click={() => (createMode = "downstream")}
          >
            {t("neuronPanel.createDownstream")}
          </button>
        </div>

        {#if createMode === "downstream"}
          <div class="modal-row">
            <label class="modal-label">{t("neuronPanel.createSource")}</label>
            <Select
              bind:value={createSource}
              placeholder={t("neuronPanel.createSourcePlaceholder")}
              options={neurons.map((n) => ({ value: n.id, label: n.desc || n.id }))}
            />
          </div>
        {/if}

        <div class="modal-row">
          <label class="modal-label">{t("neuronPanel.createDescLabel")}</label>
          <input
            class="modal-input"
            placeholder={t("neuronPanel.createDescPlaceholder")}
            bind:value={createDesc}
          />
        </div>

        <div class="modal-row">
          <label class="modal-label">{t("neuronPanel.createContentLabel")}</label>
          <textarea
            class="modal-input"
            rows="4"
            placeholder={t("neuronPanel.createContentPlaceholder")}
            bind:value={createContent}
          ></textarea>
        </div>

        <div class="modal-row">
          <label class="modal-label">{t("neuronPanel.createToolIdsLabel")}</label>
          {#if availableTools.length === 0}
            <span class="modal-hint">{t("neuronPanel.noToolsAvailable")}</span>
          {:else}
            <div class="tool-checks">
              {#each availableTools as tool (tool.name)}
                <label class="tool-check">
                  <input
                    type="checkbox"
                    checked={createToolIds.includes(tool.name)}
                    on:change={() => toggleCreateTool(tool.name)}
                  />
                  <span class="tool-name">{tool.name}</span>
                  <span class="tool-desc">{tool.description}</span>
                </label>
              {/each}
            </div>
          {/if}
        </div>

        {#if createError}
          <div class="modal-error">{createError}</div>
        {/if}

        <div class="modal-actions">
          <button class="btn-ghost" on:click={cancelCreate}>{t("neuronPanel.cancel")}</button>
          <button class="btn-primary" on:click={submitCreate} disabled={creating}>
            {creating ? t("neuronPanel.creating") : t("neuronPanel.createConfirm")}
          </button>
        </div>
      </div>
    </div>
  {/if}
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
  .core-select {
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

  .edge-type {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .layout-type {
    display: flex;
    align-items: center;
    gap: 6px;
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

  .create-btn {
    border: 1px solid var(--color-primary);
    background: var(--color-primary);
    color: var(--color-on-primary);
    border-radius: 8px;
    padding: 5px 12px;
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
    transition:
      background 0.15s ease,
      filter 0.15s ease;
  }
  .create-btn:hover {
    filter: brightness(1.08);
  }

  /* 创建弹窗 */
  .modal-mask {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .modal {
    width: 420px;
    max-width: 92vw;
    max-height: 88vh;
    overflow: auto;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 14px;
    padding: 18px 20px;
    box-shadow: 0 18px 50px rgba(0, 0, 0, 0.35);
  }
  .modal-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 14px;
  }
  .modal-row {
    margin-bottom: 12px;
  }
  .modes {
    display: flex;
    gap: 8px;
  }
  .mode-btn {
    flex: 1;
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-text-muted);
    border-radius: 8px;
    padding: 7px 10px;
    font-size: 12.5px;
    cursor: pointer;
    transition:
      background 0.15s ease,
      color 0.15s ease,
      border-color 0.15s ease;
  }
  .mode-btn.active {
    color: var(--color-on-primary);
    background: var(--color-primary);
    border-color: var(--color-primary);
  }
  .modal-label {
    display: block;
    font-size: 11.5px;
    color: var(--color-text-muted);
    margin-bottom: 5px;
  }
  .modal-input {
    width: 100%;
    box-sizing: border-box;
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 7px 10px;
    color: var(--color-text);
    font-size: 12.5px;
    font-family: inherit;
    resize: vertical;
  }
  .modal-input:focus {
    outline: none;
    border-color: var(--color-primary);
  }
  .modal-hint {
    font-size: 12px;
    color: var(--color-text-muted);
  }
  .tool-checks {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 140px;
    overflow-y: auto;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 8px;
    background: var(--color-bg);
  }
  .tool-check {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    cursor: pointer;
    font-size: 12.5px;
    line-height: 1.4;
  }
  .tool-check input {
    margin-top: 2px;
    accent-color: var(--color-primary);
  }
  .tool-name {
    font-family: var(--font-mono, monospace);
    color: var(--color-text);
    white-space: nowrap;
  }
  .tool-desc {
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .modal-error {
    color: var(--color-error);
    font-size: 12px;
    margin-bottom: 10px;
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 6px;
  }
  .btn-ghost {
    border: 1px solid var(--color-border);
    background: transparent;
    color: var(--color-text-muted);
    border-radius: 8px;
    padding: 6px 14px;
    font-size: 12.5px;
    cursor: pointer;
  }
  .btn-ghost:hover {
    color: var(--color-text);
  }
  .btn-primary {
    border: 1px solid var(--color-primary);
    background: var(--color-primary);
    color: var(--color-on-primary);
    border-radius: 8px;
    padding: 6px 16px;
    font-size: 12.5px;
    cursor: pointer;
  }
  .btn-primary:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .body {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr;
    min-height: 0;
    position: relative;
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
  }
</style>

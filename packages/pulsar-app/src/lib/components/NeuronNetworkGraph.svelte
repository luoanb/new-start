<script lang="ts">
  import {
    SvelteFlow,
    Background,
    Controls,
    NodeToolbar,
    Position,
    MarkerType,
    type Node,
    type Edge,
  } from "@xyflow/svelte";
  import { untrack } from "svelte";
  import { t } from "$lib/i18n";
  import "@xyflow/svelte/dist/style.css";
  import type { NeuronSubgraph } from "$lib/types";
  import {
    layoutRegistry,
    nodeSizeFor,
    type LayoutId,
  } from "$lib/features/neuron/networkLayout";
  import NeuronFlowNode from "./NeuronFlowNode.svelte";
  import FloatingEdge from "./FloatingEdge.svelte";

  type EdgeType = "bezier" | "smoothstep" | "step" | "straight" | "floating";

  let {
    subgraph,
    layoutId = "force",
    minW = 0,
    maxW = 1,
    onJumpTo,
    onSetSeed,
    selectedId = null,
    edgeType = "bezier",
  }: {
    subgraph: NeuronSubgraph;
    layoutId?: LayoutId;
    /** 当前 subgraph 内权重 min/max，用于节点尺寸/配色归一化（与布局同源）。 */
    minW?: number;
    maxW?: number;
    onJumpTo: (id: string) => void;
    /** 节点工具栏【设为画布核心】：点击后以该节点为根重建 subgraph。NeuronNetwork 视图无 seed 概念时不传。 */
    onSetSeed?: (id: string) => void;
    selectedId?: string | null;
    edgeType?: EdgeType;
  } = $props();

  const nodeTypes = { neuron: NeuronFlowNode };
  const edgeTypes = { floating: FloatingEdge };

  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);

  // 节点工具栏目标：点击节点展开悬浮工具栏（【选中】前不改变选中态），点击画布空白收起
  let toolbarNodeId = $state<string | null>(null);

  // 与渲染同源的尺寸函数：布局斥力/碰撞与节点渲染使用同一归一化
  const weightById = $derived(new Map(subgraph.neurons.map((n) => [n.id, n.weight])));
  const sizeOf = (id: string) => nodeSizeFor(weightById.get(id) ?? 0, minW, maxW);

  function rebuild(sg: NeuronSubgraph, lid: LayoutId) {
    const algo = layoutRegistry[lid] ?? layoutRegistry.force;
    // untrack selectedId：点击选中态不应触发布局重算（力导向 400 次迭代 O(n²)，
    // 若被 $effect 追踪会在每次点击时同步重跑，造成明显卡顿）
    const sel = untrack(() => selectedId);
    const laid = algo.run(sg, { seedId: sg.seed_id, minW, maxW, nodeSize: sizeOf });
    nodes = laid.map((n) => ({
      id: n.id,
      type: "neuron",
      position: n.position,
      data: n.data,
      draggable: true,
      selected: n.id === sel,
    }));

    const weights = sg.connections.map((c) => c.weight);
    const minWEdge = weights.length ? Math.min(...weights) : 0;
    const maxWEdge = weights.length ? Math.max(...weights) : 1;
    const span = maxWEdge - minWEdge || 1;

    edges = sg.connections.map((c) => {
      const norm = (c.weight - minWEdge) / span;
      const isFloating = edgeType === "floating";
      return {
        id: `${c.source}->${c.target}`,
        source: c.source,
        target: c.target,
        label: c.weight.toFixed(2),
        type: isFloating ? "floating" : edgeType,
        data: isFloating ? { variant: "bezier" } : undefined,
        animated: false,
        style: `stroke-width: ${1 + norm * 2.5}px`,
        markerEnd: { type: MarkerType.ArrowClosed },
      };
    });
  }

  $effect(() => {
    rebuild(subgraph, layoutId);
  });

  // 外部选中态 → 仅更新节点的 selected，且不追踪 nodes 以避免读写闭环
  $effect(() => {
    const sel = selectedId;
    untrack(() => {
      nodes = nodes.map((n) => ({ ...n, selected: n.id === sel }));
    });
  });

  // 点击节点：立即选中（selectedId + 打开抽屉），并展开节点上方悬浮工具栏
  function onNodeClick(id: string) {
    toolbarNodeId = id;
    onJumpTo(id);
  }

  function onPaneClick() {
    toolbarNodeId = null;
  }
</script>

<div class="graph-wrap">
  <SvelteFlow
    bind:nodes
    bind:edges
    {nodeTypes}
    {edgeTypes}
    fitView
    fitViewOptions={{ padding: 0.3 }}
    minZoom={0.1}
    nodesConnectable={false}
    elementsSelectable={false}
    deleteKey={null}
    onnodeclick={({ node }) => onNodeClick(node.id)}
    onpaneclick={onPaneClick}
  >
    <Background />
    <Controls />
    {#if toolbarNodeId && onSetSeed}
      <!-- 闭包内 $state 不会收窄，先在块内取值；无 onSetSeed 的视图（NeuronNetwork）不展示工具栏 -->
      {@const tid = toolbarNodeId}
      <NodeToolbar
        nodeId={tid}
        isVisible
        position={Position.Top}
        align="center"
        offset={14}
      >
        <div class="node-toolbar">
          <button class="nt-btn nt-primary" onclick={() => onSetSeed(tid)}>
            {t("neuronPanel.setAsSeed")}
          </button>
        </div>
      </NodeToolbar>
    {/if}
  </SvelteFlow>
</div>

<style>
  .graph-wrap {
    width: 100%;
    height: 100%;
    min-height: 320px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--color-bg);
  }
  .graph-wrap :global(.svelte-flow) {
    background: var(--color-bg);
  }
  /* ── xyflow Controls 主题适配（库自带 CSS 为浅色硬编码，需覆盖主题变量）── */
  .graph-wrap :global(.svelte-flow__controls) {
    box-shadow: 0 0 0 1px var(--color-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .graph-wrap :global(.svelte-flow__controls-button) {
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text);
    width: 26px;
    height: 26px;
  }
  .graph-wrap :global(.svelte-flow__controls-button:hover) {
    background: var(--color-hover);
  }
  .graph-wrap :global(.svelte-flow__controls-button svg) {
    fill: var(--color-text);
  }
  /* ── 节点悬浮工具栏（NodeToolbar 内按钮）── */
  .node-toolbar {
    display: flex;
    gap: 6px;
    padding: 4px;
    border-radius: var(--radius-md);
    background: var(--color-elevated);
    border: 1px solid var(--color-border);
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.18);
  }
  .nt-btn {
    padding: 4px 10px;
    font-size: var(--fs-sm);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
    white-space: nowrap;
    transition:
      border-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }
  .nt-btn:hover {
    border-color: var(--color-primary);
    color: var(--color-primary);
  }
  .nt-primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: var(--color-on-primary);
  }
  .nt-primary:hover {
    color: var(--color-on-primary);
    filter: brightness(1.08);
  }
</style>

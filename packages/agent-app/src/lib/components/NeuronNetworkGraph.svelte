<script lang="ts">
  import {
    SvelteFlow,
    Background,
    Controls,
    MarkerType,
    type Node,
    type Edge,
  } from "@xyflow/svelte";
  import { untrack } from "svelte";
  import "@xyflow/svelte/dist/style.css";
  import type { NeuronSubgraph } from "$lib/types";
  import { layoutForceNodes } from "$lib/features/neuron/networkLayout";
  import NeuronFlowNode from "./NeuronFlowNode.svelte";
  import FloatingEdge from "./FloatingEdge.svelte";

  type EdgeType = "bezier" | "smoothstep" | "step" | "straight" | "floating";

  let {
    subgraph,
    onJumpTo,
    selectedId = null,
    edgeType = "bezier",
  }: {
    subgraph: NeuronSubgraph;
    onJumpTo: (id: string) => void;
    selectedId?: string | null;
    edgeType?: EdgeType;
  } = $props();

  const nodeTypes = { neuron: NeuronFlowNode };
  const edgeTypes = { floating: FloatingEdge };

  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);

  function rebuild(sg: NeuronSubgraph) {
    const laid = layoutForceNodes(sg);
    nodes = laid.map((n) => ({
      id: n.id,
      type: "neuron",
      position: n.position,
      data: n.data,
      draggable: true,
      selected: n.id === selectedId,
    }));

    const weights = sg.connections.map((c) => c.weight);
    const minW = weights.length ? Math.min(...weights) : 0;
    const maxW = weights.length ? Math.max(...weights) : 1;
    const span = maxW - minW || 1;

    edges = sg.connections.map((c) => {
      const norm = (c.weight - minW) / span;
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
    rebuild(subgraph);
  });

  // 外部选中态 → 仅更新节点的 selected，且不追踪 nodes 以避免读写闭环
  $effect(() => {
    const sel = selectedId;
    untrack(() => {
      nodes = nodes.map((n) => ({ ...n, selected: n.id === sel }));
    });
  });
</script>

<div class="graph-wrap">
  <SvelteFlow
    bind:nodes
    bind:edges
    {nodeTypes}
    {edgeTypes}
    fitView
    nodesConnectable={false}
    elementsSelectable={true}
    deleteKey={null}
    onnodeclick={({ node }) => onJumpTo(node.id)}
  >
    <Background />
    <Controls />
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
</style>

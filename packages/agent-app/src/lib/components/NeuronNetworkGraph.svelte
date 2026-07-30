<script lang="ts">
  import {
    SvelteFlow,
    Background,
    Controls,
    MarkerType,
    type Node,
    type Edge,
  } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import type { NeuronSubgraph } from "$lib/types";
  import { layoutFlowNodes } from "$lib/features/neuron/networkLayout";
  import NeuronFlowNode from "./NeuronFlowNode.svelte";

  let {
    subgraph,
    onJumpTo,
  }: {
    subgraph: NeuronSubgraph;
    onJumpTo: (id: string) => void;
  } = $props();

  const nodeTypes = { neuron: NeuronFlowNode };

  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);

  function rebuild(sg: NeuronSubgraph) {
    const laid = layoutFlowNodes(sg);
    nodes = laid.map((n) => ({
      id: n.id,
      type: "neuron",
      position: n.position,
      data: n.data,
      draggable: true,
    }));

    const weights = sg.connections.map((c) => c.weight);
    const minW = weights.length ? Math.min(...weights) : 0;
    const maxW = weights.length ? Math.max(...weights) : 1;
    const span = maxW - minW || 1;

    edges = sg.connections.map((c) => {
      const norm = (c.weight - minW) / span;
      return {
        id: `${c.source}->${c.target}`,
        source: c.source,
        target: c.target,
        label: c.weight.toFixed(2),
        type: "smoothstep",
        animated: false,
        style: `stroke-width: ${1 + norm * 2.5}px`,
        markerEnd: { type: MarkerType.ArrowClosed },
      };
    });
  }

  $effect(() => {
    rebuild(subgraph);
  });
</script>

<div class="graph-wrap">
  <SvelteFlow
    bind:nodes
    bind:edges
    {nodeTypes}
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
    height: min(70vh, 560px);
    min-height: 320px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--color-bg);
  }
  .graph-wrap :global(.svelte-flow) {
    background: var(--color-bg);
  }
</style>

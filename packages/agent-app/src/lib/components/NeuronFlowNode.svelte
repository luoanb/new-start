<script lang="ts">
  import { Handle, Position } from "@xyflow/svelte";

  let {
    data,
  }: {
    data: {
      label: string;
      weight: number;
      systemType?: string | null;
      isSeed: boolean;
    };
  } = $props();
</script>

<div class="neuron-flow-node" class:is-seed={data.isSeed}>
  <Handle type="target" position={Position.Left} />
  <div class="node-label">{data.label}</div>
  <div class="node-meta">
    <span>w={data.weight.toFixed(1)}</span>
    {#if data.systemType}
      <span class="sys">{data.systemType}</span>
    {/if}
  </div>
  <Handle type="source" position={Position.Right} />
</div>

<style>
  .neuron-flow-node {
    min-width: 140px;
    max-width: 200px;
    padding: 8px 10px;
    border-radius: var(--radius-md, 8px);
    border: 1px solid var(--color-border, #444);
    background: var(--color-surface, #1e1e1e);
    color: var(--color-text, #eee);
    font-size: 12px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.2);
  }
  .neuron-flow-node.is-seed {
    border-color: var(--color-primary, #3b82f6);
    border-width: 2px;
  }
  .node-label {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .node-meta {
    margin-top: 4px;
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--color-text-muted, #999);
    font-size: 10px;
  }
  .sys {
    padding: 0 4px;
    border-radius: 4px;
    background: var(--color-primary, #3b82f6);
    color: var(--color-on-primary, #fff);
    font-weight: 600;
  }
</style>

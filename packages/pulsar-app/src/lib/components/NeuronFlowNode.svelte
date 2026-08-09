<script lang="ts">
  import { Handle, Position } from "@xyflow/svelte";

  let {
    id,
    data,
  }: {
    id: string;
    data: {
      label: string;
      weight: number;
      systemType?: string | null;
      isSeed: boolean;
      weightNorm: number;
    };
  } = $props();

  // 节点尺寸 ∝ 归一化权重（140→260），与布局/碰撞估算同源
  const width = $derived(Math.round(140 + (data.weightNorm ?? 0) * 120));
  // 权重分档配色：high ≥ 0.66 / mid ≥ 0.33 / low < 0.33
  const tier = $derived(
    data.weightNorm >= 0.66 ? "high" : data.weightNorm >= 0.33 ? "mid" : "low",
  );
</script>

<div
  class="neuron-flow-node"
  class:is-seed={data.isSeed}
  class:tier-high={tier === "high"}
  class:tier-mid={tier === "mid"}
  class:tier-low={tier === "low"}
  style="width: {width}px"
>
  <Handle id="t" type="target" position={Position.Top} />
  <Handle id="r" type="source" position={Position.Right} />
  <Handle id="b" type="target" position={Position.Bottom} />
  <Handle id="l" type="source" position={Position.Left} />
  <div class="node-label">{data.label}</div>
  <div class="node-meta">
    <span class="node-id" title={id}>{id}</span>
    <span>w={data.weight.toFixed(1)}</span>
  </div>
</div>

<style>
  .neuron-flow-node {
    min-width: 140px;
    max-width: 260px;
    padding: 8px 10px;
    border-radius: var(--radius-md, 8px);
    border: 1px solid var(--color-border, #444);
    background: var(--color-surface, #1e1e1e);
    color: var(--color-text, #eee);
    font-size: 12px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.2);
    box-sizing: border-box;
    transition: opacity 0.15s ease, filter 0.15s ease;
  }
  /* 高权重：主色系背景 + 主色边框 + 内描边 */
  .neuron-flow-node.tier-high {
    background: color-mix(in srgb, var(--color-primary) 18%, var(--color-surface));
    border-color: var(--color-primary);
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--color-primary) 35%, transparent),
      0 1px 2px rgba(0, 0, 0, 0.2);
  }
  /* 低权重：淡化退为背景 */
  .neuron-flow-node.tier-low {
    opacity: 0.72;
    filter: saturate(0.5);
  }
  /* 画布核心：粗边框 + 主色光晕，叠加在任意档位之上 */
  .neuron-flow-node.is-seed {
    border-color: var(--color-primary, #3b82f6);
    border-width: 2px;
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--color-primary) 22%, transparent);
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
  .node-id {
    flex: 1;
    min-width: 0;
    font-family: var(--font-mono, monospace);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* floating edge: hide visible handles but keep them functional */
  .neuron-flow-node :global(.svelte-flow__handle) {
    opacity: 0;
    width: 1px;
    height: 1px;
    min-width: 1px;
    min-height: 1px;
    border: none;
  }
</style>

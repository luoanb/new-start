<script lang="ts">
  import type { NeuronSubgraph } from "$lib/types";
  import { buildTreeRows } from "$lib/features/neuron/networkLayout";
  import { t } from "$lib/i18n";

  let {
    subgraph,
    onJumpTo,
  }: {
    subgraph: NeuronSubgraph;
    onJumpTo: (id: string) => void;
  } = $props();

  const rows = $derived(buildTreeRows(subgraph));
</script>

{#if rows.length === 0}
  <p class="empty">{t("neuronPanel.noNeurons")}</p>
{:else}
  <div class="network-tree">
    {#each rows as row (row.neuron.id)}
      <div
        class="tree-node"
        class:is-root={row.direction === "seed"}
        style="padding-left: {row.depth * 16}px"
      >
        <span class="depth-tag">D{row.depth}</span>
        {#if row.direction !== "seed"}
          <span class="dir" title={row.direction}>
            {row.direction === "out" ? "→" : "←"}
          </span>
        {/if}
        <button class="node-link" onclick={() => onJumpTo(row.neuron.id)}>
          {row.neuron.desc || row.neuron.id.slice(0, 20)}
        </button>
        <span class="node-meta">
          w={row.neuron.weight.toFixed(1)}
          {#if row.fromParent}
            · {t("neuronPanel.edgeWeight")}={row.fromParent.weight.toFixed(2)}
          {/if}
          · {row.neuron.id.slice(0, 8)}
        </span>
        {#if row.neuron.system_type}
          <span class="sys-tag">{row.neuron.system_type}</span>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .empty { text-align: center; color: var(--color-text-muted); font-size: var(--fs-sm); padding: var(--space-2); }
  .network-tree { display: flex; flex-direction: column; gap: 2px; }
  .tree-node {
    display: flex; align-items: center; gap: var(--space-1);
    padding: var(--space-1); border-radius: var(--radius-sm);
    background: var(--color-surface); font-size: var(--fs-xs);
  }
  .tree-node.is-root { border-left: 2px solid var(--color-primary); background: var(--color-bg); }
  .depth-tag { font-family: monospace; color: var(--color-text-muted); min-width: 1.5rem; }
  .dir { color: var(--color-primary); font-weight: 700; width: 1rem; text-align: center; }
  .node-link {
    font-size: var(--fs-sm); font-weight: 500; color: var(--color-primary);
    background: none; border: none; cursor: pointer; padding: 0; text-align: left;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 40%;
  }
  .node-link:hover { text-decoration: underline; }
  .node-meta { font-size: 10px; color: var(--color-text-muted); white-space: nowrap; }
  .sys-tag {
    font-size: 10px; font-weight: 600; padding: 1px 6px; border-radius: var(--radius-sm);
    background: var(--color-primary); color: var(--color-on-primary);
  }
</style>

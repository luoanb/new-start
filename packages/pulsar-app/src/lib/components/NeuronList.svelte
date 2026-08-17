<script lang="ts">
  import type { Neuron } from "$lib/types";
  import { t } from "$lib/i18n";

  let {
    neurons,
    onSelect,
  }: {
    neurons: Neuron[];
    onSelect: (id: string) => void;
  } = $props();

  function formatTime(ts: number): string {
    return new Date(ts).toLocaleString();
  }
</script>

<div class="neuron-list">
  {#if neurons.length === 0}
    <p class="empty">{t("neuronPanel.noNeurons")}</p>
  {:else}
    {#each neurons as n (n.id)}
      <button class="neuron-item" onclick={() => onSelect(n.id)}>
        <div class="item-top">
          <span class="item-id">{n.id.slice(0, 20)}</span>
          <span class="item-weight">{t("neuronPanel.weight")}: {n.weight.toFixed(2)}</span>
        </div>
        <div class="item-desc">{n.desc}</div>
        <div class="item-meta">
          {#if n.system_type}
            <span class="sys-tag">{n.system_type}</span>
          {/if}
          <span>{formatTime(n.created_at)}</span>
        </div>
      </button>
    {/each}
  {/if}
</div>

<style>
  .neuron-list { display: flex; flex-direction: column; gap: var(--space-1); }
  .empty { text-align: center; color: var(--color-text-muted); font-size: var(--fs-xs); padding: var(--space-4); }
  .neuron-item { display: flex; flex-direction: column; gap: 2px; padding: var(--space-2); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-md); background: var(--color-bg); text-align: left; cursor: pointer; width: 100%; color: var(--color-text); }
  .neuron-item:hover { border-color: var(--color-primary); }
  .item-top { display: flex; justify-content: space-between; align-items: center; }
  .item-id { font-size: var(--fs-xs); font-family: var(--font-mono); color: var(--color-text-muted); }
  .item-weight { font-size: var(--fs-xs); font-weight: 600; }
  .item-desc { font-size: var(--fs-xs); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .item-meta { display: flex; gap: var(--space-2); font-size: var(--fs-xs); color: var(--color-text-muted); align-items: center; }
  .sys-tag { font-size: var(--fs-xs); font-weight: 600; padding: 1px 6px; border-radius: var(--radius-sm); background: var(--color-primary); color: var(--color-on-primary); }
</style>

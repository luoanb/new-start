<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Neuron } from "$lib/types";
  import { t } from "$lib/i18n";

  let {
    rootId,
    onBack,
    onJumpTo,
  }: {
    rootId: string;
    onBack: () => void;
    onJumpTo: (id: string) => void;
  } = $props();

  let neurons = $state<Neuron[]>([]);
  let loading = $state(true);
  let errorMsg = $state("");

  async function load() {
    loading = true;
    errorMsg = "";
    try {
      neurons = await invoke<Neuron[]>("get_network", {
        id: rootId,
        max_depth: 2,
      });
    } catch (e) {
      errorMsg = `Load network failed: ${e}`;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (rootId) load();
  });

  function formatTime(ts: number): string {
    return new Date(ts).toLocaleString();
  }
</script>

<div class="neuron-network">
  <button class="back-btn" onclick={onBack}>← {t("neuronPanel.back")}</button>
  <h3>{t("neuronPanel.networkTitle")} (depth: 2)</h3>

  {#if loading}
    <p class="status-msg">{t("neuronPanel.loading")}</p>
  {:else if errorMsg}
    <div class="error-msg">{errorMsg}</div>
  {:else if neurons.length === 0}
    <p class="empty">{t("neuronPanel.noNeurons")}</p>
  {:else}
    <div class="network-tree">
      {#each neurons as n, i (n.id)}
        <div class="tree-node" class:is-root={n.id === rootId}>
          <div class="tree-indent">{#each Array(i) as _}  {/each}</div>
          <div class="tree-content">
            <button class="node-link" onclick={() => onJumpTo(n.id)}>
              {n.desc || n.id.slice(0, 20)}
            </button>
            <span class="node-meta">
              w={n.weight.toFixed(1)} &middot; {formatTime(n.created_at)}
            </span>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .neuron-network { display: flex; flex-direction: column; gap: var(--space-2); }
  .back-btn { align-self: flex-start; font-size: var(--fs-sm); padding: var(--space-1) var(--space-2); border: none; background: transparent; color: var(--color-primary); cursor: pointer; }
  .back-btn:hover { text-decoration: underline; }
  h3 { font-size: var(--fs-sm); font-weight: 600; margin: 0; }
  .status-msg, .error-msg, .empty { text-align: center; color: var(--color-text-muted); font-size: var(--fs-sm); padding: var(--space-2); }
  .error-msg { color: var(--color-danger, #ef4444); }
  .network-tree { display: flex; flex-direction: column; gap: 2px; }
  .tree-node { display: flex; align-items: center; gap: var(--space-1); padding: var(--space-1); border-radius: var(--radius-sm); background: var(--color-surface); font-size: var(--fs-xs); }
  .tree-node.is-root { border-left: 2px solid var(--color-primary); background: var(--color-bg); }
  .tree-indent { font-family: monospace; color: var(--color-text-muted); min-width: 12px; }
  .tree-content { display: flex; align-items: center; gap: var(--space-2); flex: 1; min-width: 0; }
  .node-link { font-size: var(--fs-sm); font-weight: 500; color: var(--color-primary); background: none; border: none; cursor: pointer; padding: 0; text-align: left; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .node-link:hover { text-decoration: underline; }
  .node-meta { font-size: 10px; color: var(--color-text-muted); white-space: nowrap; }
</style>

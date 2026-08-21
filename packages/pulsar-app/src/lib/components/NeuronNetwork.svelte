<script lang="ts">
  import { api, c } from "$lib/api";
  import type { NeuronSubgraph } from "$lib/types";
  import { t } from "$lib/i18n";
  import { errorMessage } from "$lib/errorMessage";
  import NeuronNetworkTree from "./NeuronNetworkTree.svelte";
  import NeuronNetworkGraph from "./NeuronNetworkGraph.svelte";
  import Select from "./Select.svelte";

  let {
    rootId,
    onBack,
    onJumpTo,
  }: {
    rootId: string;
    onBack: () => void;
    onJumpTo: (id: string) => void;
  } = $props();

  type Mode = "graph" | "tree";

  let mode = $state<Mode>("graph");
  let maxDepth = $state(2);
  let subgraph = $state<NeuronSubgraph | null>(null);
  let loading = $state(true);
  let errorMsg = $state("");

  const DEPTHS = [1, 2, 3, 4, 5];

  async function load() {
    loading = true;
    errorMsg = "";
    try {
      subgraph = await api.call(c.getNetwork, {
        id: rootId,
        max_depth: maxDepth,
      });
    } catch (e) {
      errorMsg = `Load network failed: ${errorMessage(e)}`;
      subgraph = null;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (rootId) {
      // track maxDepth + rootId
      void maxDepth;
      void rootId;
      load();
    }
  });
</script>

<div class="neuron-network">
  <div class="toolbar">
    <button class="back-btn" onclick={onBack}>← {t("neuronPanel.back")}</button>
    <h3>
      {t("neuronPanel.networkTitle").replace("{depth}", String(maxDepth))}
    </h3>
    <div class="toolbar-right">
      <label class="depth-ctrl">
        <span>{t("neuronPanel.depthLabel")}</span>
        <Select
          bind:value={maxDepth}
          options={DEPTHS.map((d) => ({ value: d, label: String(d) }))}
        />
      </label>
      <div class="mode-toggle">
        <button
          class:active={mode === "graph"}
          onclick={() => (mode = "graph")}
        >
          {t("neuronPanel.viewModeGraph")}
        </button>
        <button
          class:active={mode === "tree"}
          onclick={() => (mode = "tree")}
        >
          {t("neuronPanel.viewModeTree")}
        </button>
      </div>
    </div>
  </div>

  {#if loading}
    <p class="status-msg">{t("neuronPanel.loading")}</p>
  {:else if errorMsg}
    <div class="error-msg">{errorMsg}</div>
  {:else if !subgraph || subgraph.neurons.length === 0}
    <p class="empty">{t("neuronPanel.noNeurons")}</p>
  {:else if mode === "graph"}
    <NeuronNetworkGraph {subgraph} {onJumpTo} />
  {:else}
    <NeuronNetworkTree {subgraph} {onJumpTo} />
  {/if}
</div>

<style>
  .neuron-network { display: flex; flex-direction: column; gap: var(--space-2); }
  .toolbar {
    display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-2);
  }
  .toolbar-right {
    margin-left: auto; display: flex; align-items: center; gap: var(--space-2);
  }
  .back-btn {
    font-size: var(--fs-xs); padding: var(--space-1) var(--space-2);
    border: none; background: transparent; color: var(--color-primary); cursor: pointer;
  }
  .back-btn:hover { text-decoration: underline; }
  h3 { font-size: var(--fs-xs); font-weight: 600; margin: 0; }
  .depth-ctrl {
    display: flex; align-items: center; gap: var(--space-1);
    font-size: var(--fs-xs); color: var(--color-text-muted);
  }
  .mode-toggle { display: flex; border: 1px solid var(--color-border); border-radius: var(--radius-sm); overflow: hidden; }
  .mode-toggle button {
    font-size: var(--fs-xs); padding: 4px 10px; border: none;
    background: var(--color-surface); color: var(--color-text-muted); cursor: pointer;
  }
  .mode-toggle button.active {
    background: var(--color-primary); color: var(--color-on-primary, #fff); font-weight: 600;
  }
  .status-msg, .error-msg, .empty {
    text-align: center; color: var(--color-text-muted); font-size: var(--fs-xs); padding: var(--space-2);
  }
  .error-msg { color: var(--color-error); }
</style>

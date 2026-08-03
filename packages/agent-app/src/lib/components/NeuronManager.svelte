<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Neuron } from "$lib/types";
  import { t } from "$lib/i18n";
  import { errorMessage } from "$lib/errorMessage";
  import NeuronList from "./NeuronList.svelte";
  import NeuronDetail from "./NeuronDetail.svelte";
  import NeuronNetwork from "./NeuronNetwork.svelte";

  type ViewState = "list" | "detail" | "network";

  let view = $state<ViewState>("list");
  let neurons = $state<Neuron[]>([]);
  let activeNeuronId = $state<string>("");
  let loading = $state(true);
  let errorMsg = $state("");

  async function loadList() {
    loading = true;
    errorMsg = "";
    try {
      neurons = await invoke<Neuron[]>("list_neurons");
      // Sort by weight descending
      neurons.sort((a, b) => b.weight - a.weight);
    } catch (e) {
      errorMsg = `Failed to load neurons: ${errorMessage(e)}`;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (view === "list") loadList();
  });

  function goDetail(id: string) {
    activeNeuronId = id;
    view = "detail";
  }

  function goNetwork(id: string) {
    activeNeuronId = id;
    view = "network";
  }

  function goBack() {
    view = "list";
  }
</script>

<div class="neuron-manager">
  <div class="manager-header">
    <h2>{t("neuronPanel.neurons")}</h2>
  </div>

  {#if errorMsg}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="error-banner" onclick={() => (errorMsg = "")}>{errorMsg}</div>
  {/if}

  {#if view === "list"}
    {#if loading}
      <p class="status-msg">{t("neuronPanel.loading")}</p>
    {:else}
      <NeuronList {neurons} onSelect={goDetail} />
    {/if}
  {:else if view === "detail"}
    <NeuronDetail
      neuronId={activeNeuronId}
      onBack={goBack}
      onViewNetwork={goNetwork}
      onJumpTo={goDetail}
    />
  {:else if view === "network"}
    <NeuronNetwork
      rootId={activeNeuronId}
      onBack={goBack}
      onJumpTo={goDetail}
    />
  {/if}
</div>

<style>
  .neuron-manager { display: flex; flex-direction: column; gap: var(--space-2); height: 100%; overflow-y: auto; padding: var(--space-3); }
  .manager-header h2 { margin: 0; font-size: var(--fs-base); font-weight: 600; }
  .error-banner { background: var(--color-danger, #ef4444); color: #fff; padding: var(--space-1) var(--space-2); border-radius: var(--radius-md); font-size: var(--fs-xs); cursor: pointer; }
  .status-msg { text-align: center; color: var(--color-text-muted); font-size: var(--fs-sm); padding: var(--space-4); }
</style>

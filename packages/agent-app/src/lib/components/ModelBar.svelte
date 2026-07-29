<script lang="ts">
  import type { ProviderInfo, ModelInfo } from "$lib/types";

  let {
    providers,
    totalModels = 0,
    visibleModels,
    selectedProviderId,
    selectedModelId,
    onChange,
  }: {
    providers: ProviderInfo[];
    totalModels?: number;
    visibleModels: ModelInfo[];
    selectedProviderId: string;
    selectedModelId: string;
    onChange: (providerId: string, modelId: string) => void;
  } = $props();

  function handleProviderChange(e: Event) {
    const val = (e.target as HTMLSelectElement).value;
    onChange(val, "");
  }

  function handleModelChange(e: Event) {
    const val = (e.target as HTMLSelectElement).value;
    onChange(selectedProviderId, val);
  }
</script>

<div class="model-bar">
  <div class="select-group">
    <label for="provider-select">Provider</label>
    <select id="provider-select" value={selectedProviderId} onchange={handleProviderChange}>
      <option value="">Select provider</option>
      {#each providers as p}
        <option value={p.id}>{p.display_name}</option>
      {/each}
    </select>
  </div>

  <div class="select-group">
    <label for="model-select">Model <span class="model-count">({visibleModels.length})</span></label>
    <span class="debug-info">(total: {totalModels})</span>
    <select
      id="model-select"
      value={selectedModelId}
      onchange={handleModelChange}
      disabled={!selectedProviderId}
    >
      <option value="">Select model</option>
      {#each visibleModels as m}
        <option value={m.id}>
          {m.display_name}
        </option>
      {/each}
    </select>
  </div>
</div>

<style>
  .model-bar {
    display: flex;
    gap: 12px;
    padding: 8px 16px;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
  }

  .select-group {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .select-group label {
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  select {
    padding: 4px 10px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    font-size: 13px;
    background: var(--color-bg);
    color: var(--color-text);
    outline: none;
    cursor: pointer;
    max-width: 200px;
  }

  select:focus {
    border-color: var(--color-primary);
  }

  select:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .model-count {
    font-size: 11px;
    opacity: 0.6;
    font-weight: 400;
  }
</style>

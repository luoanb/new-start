<script lang="ts">
  import type { ProviderInfo, ModelInfo } from "$lib/types";
  import { t } from "$lib/i18n";

  let {
    providers = [],
    models = [],
    selectedProviderId,
    selectedModelId,
    onChange,
  }: {
    providers?: ProviderInfo[];
    models?: ModelInfo[];
    selectedProviderId: string;
    selectedModelId: string;
    onChange?: (providerId: string, modelId: string) => void;
  } = $props();

  let modelOpen = $state(false);

  let selectedProvider = $derived(
    providers.find((p) => p.id === selectedProviderId)
  );

  let selectedModel = $derived(
    models.find((m) => m.id === selectedModelId && m.provider_id === selectedProviderId)
  );

  let modelLabel = $derived(
    selectedProvider && selectedModel
      ? `${selectedProvider.display_name} / ${selectedModel.display_name}`
      : selectedProviderId && selectedModelId
        ? `${selectedProviderId}/${selectedModelId}`
        : t("common.noModel")
  );
</script>

<div class="model-picker">
  <button
    class="model-trigger"
    onclick={() => (modelOpen = !modelOpen)}
    class:no-model={!selectedProviderId}
    title={t("common.noModel")}
  >
    <span class="model-label">{modelLabel}</span>
    <span class="arrow">{modelOpen ? "▴" : "▾"}</span>
  </button>

  {#if modelOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="model-backdrop" role="presentation" onclick={() => (modelOpen = false)}></div>
    <div class="model-dropdown">
      {#if providers.length === 0}
        <div class="dropdown-empty">{t("sidePanel.noProviders")}</div>
      {:else}
        {#each providers as p}
          <div class="provider-group">
            <div class="provider-name">{p.display_name}</div>
            {#each models.filter((m) => m.provider_id === p.id) as m}
              <button
                class="model-option"
                class:active={selectedProviderId === p.id && selectedModelId === m.id}
                onclick={() => {
                  onChange?.(p.id, m.id);
                  modelOpen = false;
                }}
              >
                <span class="model-option-name">{m.display_name}</span>
                <span class="model-option-caps">
                  {#if m.capabilities.tools}tools {/if}
                  {#if m.context_window}({m.context_window}ctx){/if}
                </span>
              </button>
            {/each}
          </div>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .model-picker { position: relative; }

  .model-trigger {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 3px var(--space-2);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    color: var(--color-text);
    font-size: var(--fs-xs);
    font-family: var(--font-mono, monospace);
    cursor: pointer;
    white-space: nowrap;
    transition: border-color var(--duration-fast) var(--ease-out);
    max-width: 220px;
  }

  .model-trigger:hover { border-color: var(--color-primary); }
  .model-trigger.no-model { color: var(--color-error); border-color: var(--color-error); }

  .model-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .arrow { font-size: 10px; flex-shrink: 0; }

  .model-backdrop { position: fixed; inset: 0; z-index: 10; }

  .model-dropdown {
    position: absolute;
    bottom: calc(100% + var(--space-1));
    left: 0;
    z-index: 20;
    min-width: 240px;
    max-height: 320px;
    overflow-y: auto;
    background: var(--color-elevated);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.12);
  }

  .dropdown-empty {
    padding: var(--space-3) var(--space-4);
    color: var(--color-text-muted);
    font-size: var(--fs-sm);
  }

  .provider-group { border-bottom: var(--border-width) solid var(--color-border); }
  .provider-group:last-child { border-bottom: none; }

  .provider-name {
    padding: var(--space-1) var(--space-3);
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-text-muted);
    background: var(--color-surface);
  }

  .model-option {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-sm);
    cursor: pointer;
    text-align: left;
    transition: background var(--duration-fast) var(--ease-out);
  }

  .model-option:hover { background: var(--color-hover); }
  .model-option.active { background: var(--color-primary); color: var(--color-on-primary); }
  .model-option-name { font-weight: 500; }
  .model-option-caps { font-size: var(--fs-xs); opacity: 0.6; }
  .model-option.active .model-option-caps { opacity: 0.8; }
</style>

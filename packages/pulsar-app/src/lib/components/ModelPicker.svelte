<script lang="ts">
  import type { ProviderInfo, ModelInfo, SamplingParams, ThinkingConfig, ThinkingEffort } from "$lib/types";
  import { t } from "$lib/i18n";

  let {
    providers = [],
    models = [],
    selectedProviderId,
    selectedModelId,
    params,
    thinking,
    onChange,
  }: {
    providers?: ProviderInfo[];
    models?: ModelInfo[];
    selectedProviderId: string;
    selectedModelId: string;
    params?: SamplingParams;
    thinking?: ThinkingConfig;
    onChange?: (providerId: string, modelId: string, params?: SamplingParams, thinking?: ThinkingConfig) => void;
  } = $props();

  let modelOpen = $state(false);
  let paramsOpen = $state(false);

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

  // 模型是否声明支持思考模式（providers 抹平能力声明，对齐后端 ThinkingCapability）。
  let thinkingSupported = $derived(
    !!selectedModel?.thinking?.supported
  );

  let thinkingEnabled = $derived(
    thinking?.enabled ?? selectedModel?.thinking?.default_enabled ?? false
  );

  let effort = $derived(
    thinking?.effort ?? selectedModel?.thinking?.default_effort
  );

  let temperature = $state<string>(params?.temperature?.toString() ?? "");
  let topP = $state<string>(params?.top_p?.toString() ?? "");
  let maxTokens = $state<string>(params?.max_tokens?.toString() ?? "");
  let presencePenalty = $state<string>(params?.presence_penalty?.toString() ?? "");
  let frequencyPenalty = $state<string>(params?.frequency_penalty?.toString() ?? "");

  $effect(() => {
    temperature = params?.temperature?.toString() ?? "";
    topP = params?.top_p?.toString() ?? "";
    maxTokens = params?.max_tokens?.toString() ?? "";
    presencePenalty = params?.presence_penalty?.toString() ?? "";
    frequencyPenalty = params?.frequency_penalty?.toString() ?? "";
  });

  function emitParams() {
    const next: SamplingParams = {};
    if (temperature !== "") next.temperature = parseFloat(temperature);
    if (topP !== "") next.top_p = parseFloat(topP);
    if (maxTokens !== "") next.max_tokens = parseInt(maxTokens, 10);
    if (presencePenalty !== "") next.presence_penalty = parseFloat(presencePenalty);
    if (frequencyPenalty !== "") next.frequency_penalty = parseFloat(frequencyPenalty);
    const empty = Object.keys(next).length === 0;
    // 思考模式：仅当模型支持时携带；enabled 显式 true/false（避免不勾选回落 undefined，
    // 被服务商当作"未设置"而默认开启深度思考，如 DeepSeek 默认 enabled）。
    let nextThinking: ThinkingConfig | undefined;
    if (thinkingSupported) {
      nextThinking = { enabled: thinkingEnabled, effort };
    }
    onChange?.(
      selectedProviderId,
      selectedModelId,
      empty ? undefined : next,
      nextThinking,
    );
  }

  function selectModel(providerId: string, modelId: string) {
    modelOpen = false;
    // 切模型时沿用当前参数/思考设置，落库新模型。
    onChange?.(providerId, modelId, params, thinking);
  }
</script>

<div class="model-picker">
  <button
    class="model-trigger"
    onclick={() => { modelOpen = !modelOpen; paramsOpen = false; }}
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
                onclick={() => selectModel(p.id, m.id)}
              >
                <span class="model-option-name">{m.display_name}</span>
                <span class="model-option-caps">
                  {#if m.capabilities.tools}tools {/if}
                  {#if m.thinking?.supported}thinking {/if}
                  {#if m.context_window}({m.context_window}ctx){/if}
                </span>
              </button>
            {/each}
          </div>
        {/each}
      {/if}
    </div>
  {/if}

  <!-- 高级参数（采样 + 思考模式）：会话级覆盖，随选择落库后端 -->
  <button
    class="params-trigger"
    title="模型参数"
    aria-label="模型参数"
    onclick={() => { paramsOpen = !paramsOpen; modelOpen = false; }}
  >
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M3 6h18M7 12h10M10 18h4"/>
    </svg>
  </button>

  {#if paramsOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="model-backdrop" role="presentation" onclick={() => (paramsOpen = false)}></div>
    <div class="params-dropdown">
      <div class="params-header">模型参数</div>

      <div class="params-group">
        <div class="params-row">
          <label>思考模式（深度思考）</label>
          <input
            type="checkbox"
            disabled={!thinkingSupported}
            checked={thinkingEnabled}
            onchange={(e) => {
              thinkingEnabled = e.currentTarget.checked;
              emitParams();
            }}
          />
        </div>
        {#if thinkingSupported}
          <div class="params-row">
            <label>思考强度</label>
            <select
              value={effort ?? ""}
              onchange={(e) => {
                const v = e.currentTarget.value;
                effort = (v === "low" || v === "high" || v === "max") ? v : undefined;
                emitParams();
              }}
            >
              <option value="">默认</option>
              <option value="low">low</option>
              <option value="high">high</option>
              <option value="max">max</option>
            </select>
          </div>
        {:else}
          <div class="params-hint">该模型不支持思考模式</div>
        {/if}
      </div>

      <div class="params-group">
        <div class="params-row">
          <label>temperature</label>
          <input type="number" step="0.1" min="0" max="2" placeholder="默认" bind:value={temperature} onchange={emitParams} />
        </div>
        <div class="params-row">
          <label>top_p</label>
          <input type="number" step="0.05" min="0" max="1" placeholder="默认" bind:value={topP} onchange={emitParams} />
        </div>
        <div class="params-row">
          <label>max_tokens</label>
          <input type="number" step="1" min="1" placeholder="默认" bind:value={maxTokens} onchange={emitParams} />
        </div>
        <div class="params-row">
          <label>presence_penalty</label>
          <input type="number" step="0.1" min="-2" max="2" placeholder="默认" bind:value={presencePenalty} onchange={emitParams} />
        </div>
        <div class="params-row">
          <label>frequency_penalty</label>
          <input type="number" step="0.1" min="-2" max="2" placeholder="默认" bind:value={frequencyPenalty} onchange={emitParams} />
        </div>
      </div>

      <div class="params-hint">
        采样参数在思考模式开启时可能不生效（以服务商为准）；改动实时应用到当前会话。
      </div>
    </div>
  {/if}
</div>

<style>
  .model-picker { position: relative; display: inline-flex; align-items: center; gap: var(--space-1); }

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

  .params-trigger {
    display: inline-flex; align-items: center; justify-content: center;
    width: 22px; height: 22px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    color: var(--color-text-muted);
    cursor: pointer;
    transition: border-color var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out);
  }
  .params-trigger:hover { border-color: var(--color-primary); color: var(--color-primary); }

  .model-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .arrow { font-size: 10px; flex-shrink: 0; }

  .model-backdrop { position: fixed; inset: 0; z-index: 10; }

  .model-dropdown,
  .params-dropdown {
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

  .params-dropdown { width: 280px; max-height: 400px; padding: var(--space-2); }

  .params-header { font-size: var(--fs-xs); font-weight: 600; color: var(--color-text-muted); text-transform: uppercase; letter-spacing: 0.04em; margin-bottom: var(--space-2); }

  .params-group { margin-bottom: var(--space-2); }
  .params-group:last-of-type { margin-bottom: 0; }

  .params-row { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); margin-bottom: var(--space-1); }
  .params-row label { font-size: var(--fs-xs); color: var(--color-text); flex-shrink: 0; }
  .params-row input[type="number"], .params-row select {
    width: 110px; padding: 2px var(--space-1);
    border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm);
    background: var(--color-bg); color: var(--color-text); font-size: var(--fs-xs);
  }

  .params-hint { font-size: var(--fs-xs); color: var(--color-text-muted); margin-top: var(--space-1); }

  .dropdown-empty { padding: var(--space-3) var(--space-4); color: var(--color-text-muted); font-size: var(--fs-sm); }

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

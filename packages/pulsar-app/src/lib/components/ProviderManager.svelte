<script lang="ts">
  import { onMount } from "svelte";
  import { api, c } from "$lib/api";
  import { t } from "$lib/i18n";
  import Select from "./Select.svelte";
  import Toggle from "./Toggle.svelte";
  import Tooltip from "./Tooltip.svelte";
  import type {
    ModelCapabilities,
    ModelEditInfo,
    ProviderConfigView,
    ProviderEditInfo,
  } from "$lib/types";
  import { useViewContext } from "$lib/layout/viewContext";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";

  const ctx = useViewContext();
  const data = ctx.stores.data;

  let loading = $state(true);
  let saving = $state(false);
  let error = $state("");
  let view = $state<ProviderConfigView>({ providers: [] });
  let selectedId = $state<string | null>(null);

  const selected = $derived(
    view.providers.find((p) => p.id === selectedId) ?? null,
  );
  const creating = $derived(selectedId === "");

  const kindOptions = [
    { value: "open_ai", label: "open_ai" },
    { value: "open_ai_compatible", label: "open_ai_compatible" },
  ];

  type CapKey = Exclude<keyof ModelCapabilities, "extras">;
  const capKeys: { key: CapKey; label: string }[] = [
    { key: "chat", label: t("sidePanel.caps.chat") },
    { key: "tools", label: t("sidePanel.caps.tools") },
    { key: "streaming", label: t("sidePanel.caps.streaming") },
    { key: "structured_output", label: t("sidePanel.caps.structured_output") },
    { key: "vision", label: t("sidePanel.caps.vision") },
  ];

  onMount(load);

  async function load() {
    loading = true;
    error = "";
    try {
      view = await api.call(c.getProviderConfig, undefined);
      // 默认选中第一个启用（未禁用）的服务商
      selectedId =
        view.providers.find((p) => p.enabled)?.id ?? view.providers[0]?.id ?? null;
    } catch (e) {
      error = formatInvokeError(e);
    } finally {
      loading = false;
    }
  }

  function closeEditor() {
    if (saving) return;
    ctx.commands.closeProviderManager();
  }

  async function saveConfig() {
    if (saving) return;
    saving = true;
    error = "";
    try {
      const saved = await api.call(c.saveProviderConfig, { view });
      view = saved;
      await ctx.commands.closeProviderManager();
    } catch (e) {
      error = formatInvokeError(e);
    } finally {
      saving = false;
    }
  }

  // ── 列表 ←→ 表单共享状态（面板「编辑」/「新增」→ 编辑器打开对应服务商）──

  let lastEditRequestId = $state<string | null>(null);
  $effect(() => {
    const req = data.state.providerEditRequestId;
    if (!req || req === lastEditRequestId) return;
    if (!view.providers.some((p) => p.id === req)) return;
    lastEditRequestId = req;
    data.state.providerEditRequestId = null;
    selectedId = req;
  });

  let lastCreateRequest = $state(0);
  $effect(() => {
    const req = data.state.providerCreateRequest;
    if (req === lastCreateRequest) return;
    lastCreateRequest = req;
    // 已有未保存的新建草稿则跳过，仅切换选中
    if (view.providers.some((p) => p.id === "")) {
      selectedId = "";
      return;
    }
    view.providers = [...view.providers, newProvider()];
    selectedId = "";
  });

  // ── 服务商表单操作 ──

  function newProvider(): ProviderEditInfo {
    return {
      id: "",
      display_name: "",
      kind: "open_ai_compatible",
      api_base: "",
      api_key: "",
      api_key_set: false,
      auth_env: "",
      enabled: true,
      builtin: false,
      models: [],
    };
  }

  function newModel(): ModelEditInfo {
    return {
      id: "",
      display_name: "",
      capabilities: { chat: true, tools: false, streaming: true },
      context_window: null,
      max_output_tokens: null,
      sampling: null,
      thinking: null,
      pricing_input: null,
      pricing_output: null,
    };
  }

  function removeProvider(id: string) {
    const idx = view.providers.findIndex((p) => p.id === id);
    if (idx < 0) return;
    view.providers.splice(idx, 1);
    if (selectedId === id) selectedId = null;
  }

  // ── 默认模型设置 ──

  const defaultProviderOptions = $derived(
    view.providers
      .filter((p) => p.enabled)
      .map((p) => ({ value: p.id, label: p.display_name || p.id })),
  );

  const defaultModelOptions = $derived.by(() => {
    const p = view.providers.find((x) => x.id === view.defaults?.provider);
    return p
      ? p.models.map((m) => ({ value: m.id, label: m.display_name || m.id }))
      : [];
  });

  function setDefaultProvider(v: string) {
    if (!v) {
      view.defaults = null;
      return;
    }
    const p = view.providers.find((x) => x.id === v);
    const keepModel =
      view.defaults?.provider === v ? view.defaults.model : "";
    const model = p?.models.some((m) => m.id === keepModel)
      ? keepModel
      : (p?.models[0]?.id ?? "");
    view.defaults = model ? { provider: v, model } : null;
  }

  function setDefaultModel(v: string) {
    if (!view.defaults) return;
    if (!v) {
      view.defaults = null;
      return;
    }
    view.defaults = { provider: view.defaults.provider, model: v };
  }

  function clearDefaults() {
    view.defaults = null;
  }
</script>

<div class="provider-manager">
  <div class="manager-header">
    <span class="manager-title">{t("providerManager.modalTitle")}</span>
  </div>

  {#if error}
    <button class="error-banner" type="button" onclick={() => (error = "")}>
      <span class="error-text">{error}</span>
      <span class="error-dismiss" aria-hidden="true">×</span>
    </button>
  {/if}

  {#if loading}
    <p class="empty">{t("providerManager.loading")}</p>
  {:else}
    <!-- 默认模型（全局）：不依赖左侧选中项，独立于服务商编辑区 -->
    <div class="defaults-section">
      <div class="form-section-title">
        <span>{t("providerManager.defaultModel")}</span>
        {#if view.defaults}
          <button
            class="text-btn"
            type="button"
            onclick={clearDefaults}
            aria-label={t("providerManager.defaultClear")}
          >
            {t("providerManager.defaultClear")}
          </button>
        {/if}
      </div>
      <div class="field-row">
        <label class="field">
          <span class="field-label">{t("providerManager.defaultProvider")}</span>
          <Select
            value={view.defaults?.provider ?? ""}
            options={defaultProviderOptions}
            placeholder={t("providerManager.noDefaults")}
            onchange={(v) => setDefaultProvider(v as string)}
          />
        </label>
        <label class="field">
          <span class="field-label">{t("providerManager.defaultModel")}</span>
          <Select
            value={view.defaults?.model ?? ""}
            options={defaultModelOptions}
            placeholder={t("providerManager.noDefaults")}
            disabled={!view.defaults?.provider}
            onchange={(v) => setDefaultModel(v as string)}
          />
        </label>
      </div>
    </div>

    <div class="manager-body">
      <!-- 左：服务商列表 -->
      <div class="provider-list">
        <div class="list-header">
          <span>{t("providerManager.providers")}</span>
          <Tooltip label={t("providerManager.addProvider")}>
            <button
              class="icon-btn"
              type="button"
              onclick={() => {
                if (view.providers.some((p) => p.id === "")) {
                  selectedId = "";
                  return;
                }
                view.providers = [...view.providers, newProvider()];
                selectedId = "";
              }}
              aria-label={t("providerManager.addProvider")}
            >
              <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="12" y1="5" x2="12" y2="19" />
                <line x1="5" y1="12" x2="19" y2="12" />
              </svg>
            </button>
          </Tooltip>
        </div>
        <div class="list-items">
          {#each view.providers as p (p.id || "new")}
            <button
              class="list-item"
              class:active={p.id === selectedId}
              onclick={() => (selectedId = p.id)}
            >
              <span class="list-item-name" title={p.display_name ?? p.id}>
                {p.display_name || p.id || t("providerManager.untitled")}
              </span>
              {#if !p.enabled}
                <span class="badge disabled">{t("providerManager.disabled")}</span>
              {/if}
              {#if p.builtin}
                <span class="badge builtin">{t("providerManager.builtin")}</span>
              {/if}
            </button>
          {/each}
          {#if view.providers.length === 0}
            <p class="list-empty">{t("providerManager.noProviders")}</p>
          {/if}
        </div>
      </div>

      <!-- 右：服务商表单 -->
      <div class="provider-form">
        {#if selected}
          <div class="form-section">
            <div class="form-section-title">{t("providerManager.providerFields")}</div>
            <div class="field-row">
              <label class="field">
                <span class="field-label">{t("providerManager.id")}</span>
                <input
                  type="text"
                  bind:value={selected.id}
                  disabled={selected.builtin}
                  placeholder="my-provider"
                />
              </label>
              <label class="field">
                <span class="field-label">{t("providerManager.displayName")}</span>
                <input type="text" bind:value={selected.display_name} placeholder="My Provider" />
              </label>
            </div>
            <div class="field-row">
              <label class="field">
                <span class="field-label">{t("providerManager.kind")}</span>
                <Select
                  value={selected.kind}
                  options={kindOptions}
                  disabled={selected.builtin}
                  onchange={(v) => (selected.kind = v as ProviderEditInfo["kind"])}
                />
              </label>
              <label class="field">
                <span class="field-label">{t("providerManager.apiBase")}</span>
                <input
                  type="text"
                  bind:value={selected.api_base}
                  placeholder="https://api.example.com/v1"
                />
              </label>
              <label class="field">
                <span class="field-label">{t("providerManager.authEnv")}</span>
                <input
                  type="text"
                  bind:value={selected.auth_env}
                  placeholder="MY_API_KEY"
                />
              </label>
            </div>
            <div class="field-row">
              <label class="field">
                <span class="field-label">{t("providerManager.apiKey")}</span>
                <input
                  type="password"
                  bind:value={selected.api_key}
                  placeholder={selected.api_key_set ? t("providerManager.apiKeyMasked") : t("providerManager.apiKeyPlaceholder")}
                />
              </label>
              <div class="field-toggle">
                <Toggle bind:checked={selected.enabled} label={t("providerManager.enabled")} />
              </div>
            </div>
          </div>

          <!-- 模型编辑 -->
          <div class="form-section">
            <div class="form-section-title">
              <span>{t("providerManager.models")}</span>
              <Tooltip label={t("providerManager.addModel")}>
                <button
                  class="icon-btn"
                  type="button"
                  onclick={() => selected.models.push(newModel())}
                  aria-label={t("providerManager.addModel")}
                >
                  <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <line x1="12" y1="5" x2="12" y2="19" />
                    <line x1="5" y1="12" x2="19" y2="12" />
                  </svg>
                </button>
              </Tooltip>
            </div>
            {#if selected.models.length === 0}
              <p class="empty">{t("providerManager.noModels")}</p>
            {:else}
              {#each selected.models as m, i (i)}
                <div class="model-item">
                  <div class="field-row">
                    <label class="field">
                      <span class="field-label">{t("providerManager.id")}</span>
                      <input type="text" bind:value={m.id} placeholder="gpt-4o" />
                    </label>
                    <label class="field">
                      <span class="field-label">{t("providerManager.displayName")}</span>
                      <input type="text" bind:value={m.display_name} placeholder="GPT-4o" />
                    </label>
                    <div class="field-narrow">
                      <span class="field-label">{t("providerManager.contextWindow")}</span>
                      <input type="number" bind:value={m.context_window} placeholder="128000" />
                    </div>
                    <div class="field-narrow">
                      <span class="field-label">{t("providerManager.maxOutput")}</span>
                      <input type="number" bind:value={m.max_output_tokens} placeholder="8192" />
                    </div>
                  </div>
                  <div class="field-row">
                    <div class="field-narrow">
                      <span class="field-label">{t("providerManager.priceIn")}</span>
                      <input type="number" step="0.0001" bind:value={m.pricing_input} placeholder="0" />
                    </div>
                    <div class="field-narrow">
                      <span class="field-label">{t("providerManager.priceOut")}</span>
                      <input type="number" step="0.0001" bind:value={m.pricing_output} placeholder="0" />
                    </div>
                    <div class="caps-group">
                      {#each capKeys as cap}
                        <label class="cap-check">
                          <input
                            type="checkbox"
                            checked={m.capabilities[cap.key]}
                            onchange={(e) => (m.capabilities[cap.key] = e.currentTarget.checked)}
                          />
                          {cap.label}
                        </label>
                      {/each}
                    </div>
                  </div>
                  <div class="field-row">
                    <div class="field-narrow">
                      <span class="field-label">默认温度</span>
                      <input
                        type="number"
                        step="0.1"
                        min="0"
                        max="2"
                        placeholder="默认"
                        value={m.sampling?.temperature ?? null}
                        onchange={(e) => {
                          const v = e.currentTarget.value;
                          if (!m.sampling) m.sampling = {};
                          m.sampling.temperature = v === "" ? undefined : parseFloat(v);
                        }}
                      />
                    </div>
                    <div class="field-narrow">
                      <span class="field-label">默认 top_p</span>
                      <input
                        type="number"
                        step="0.05"
                        min="0"
                        max="1"
                        placeholder="默认"
                        value={m.sampling?.top_p ?? null}
                        onchange={(e) => {
                          const v = e.currentTarget.value;
                          if (!m.sampling) m.sampling = {};
                          m.sampling.top_p = v === "" ? undefined : parseFloat(v);
                        }}
                      />
                    </div>
                    <div class="field-narrow">
                      <span class="field-label">支持思考模式</span>
                      <input
                        type="checkbox"
                        checked={m.thinking?.supported ?? false}
                        onchange={(e) => {
                          if (!m.thinking) m.thinking = { supported: false };
                          m.thinking.supported = e.currentTarget.checked;
                        }}
                      />
                    </div>
                    <div class="model-actions">
                      <Tooltip label={t("providerManager.deleteModel")} position="top">
                        <button
                          class="icon-btn danger"
                          type="button"
                          onclick={() => selected.models.splice(i, 1)}
                          aria-label={t("providerManager.deleteModel")}
                        >
                          <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <polyline points="3 6 5 6 21 6" />
                            <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                            <line x1="10" y1="11" x2="10" y2="17" />
                            <line x1="14" y1="11" x2="14" y2="17" />
                          </svg>
                        </button>
                      </Tooltip>
                    </div>
                  </div>
                </div>
              {/each}
            {/if}
          </div>

          <div class="form-section">
            <div class="form-section-title">{t("providerManager.deleteProvider")}</div>
            {#if selected.builtin}
              <p class="form-hint">{t("providerManager.builtinDeleteHint")}</p>
              <button
                class="btn danger-outline"
                type="button"
                onclick={() => (selected.enabled = false)}
                disabled={!selected.enabled}
              >
                {t("providerManager.disableProvider")}
              </button>
            {:else}
              <p class="form-hint">{t("providerManager.customDeleteHint")}</p>
              <button
                class="btn danger-outline"
                type="button"
                onclick={() => removeProvider(selected.id)}
                disabled={!selected.id}
              >
                {t("providerManager.deleteProvider")}
              </button>
            {/if}
          </div>
        {:else}
          <p class="empty">{t("providerManager.selectProvider")}</p>
        {/if}
      </div>
    </div>

    <div class="manager-footer">
      <button class="btn" type="button" onclick={closeEditor} disabled={saving}>
        {t("providerManager.cancel")}
      </button>
      <button class="btn primary" type="button" onclick={saveConfig} disabled={saving || creating}>
        {saving ? t("providerManager.saving") : t("providerManager.save")}
      </button>
    </div>
  {/if}
</div>

<style>
  .provider-manager {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    overflow: hidden;
  }

  .manager-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }
  .manager-title {
    font-size: var(--fs-lg);
    font-weight: 600;
    color: var(--color-text);
  }

  .icon-btn {
    width: 26px;
    height: 26px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: background var(--duration-fast) var(--ease-out),
                color var(--duration-fast) var(--ease-out);
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--color-hover);
    color: var(--color-text);
  }
  .icon-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .icon-btn.danger {
    color: var(--color-error);
  }
  .icon-btn .icon {
    display: block;
  }

  .error-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    width: 100%;
    text-align: left;
    border: none;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-error-bg);
    color: var(--color-error);
    font-size: var(--fs-xs);
    cursor: pointer;
  }
  .error-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .error-dismiss {
    flex-shrink: 0;
    font-weight: 600;
    opacity: 0.7;
  }

  .empty {
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    padding: var(--space-4) 0;
  }

  .manager-body {
    display: flex;
    flex: 1;
    min-height: 0;
    gap: var(--space-3);
    overflow: hidden;
  }

  /* 默认模型（全局）设置区 */
  .defaults-section {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    padding: var(--space-3);
  }
  .defaults-section .field-row {
    align-items: flex-end;
  }
  .text-btn {
    border: none;
    background: transparent;
    color: var(--color-primary);
    font-size: var(--fs-xs);
    cursor: pointer;
    padding: 0;
  }
  .text-btn:hover {
    text-decoration: underline;
  }

  /* 左侧服务商列表 */
  .provider-list {
    flex: 0 0 220px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    overflow: hidden;
  }
  .list-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-2) var(--space-3);
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--color-text);
    border-bottom: var(--border-width) solid var(--color-border);
    flex-shrink: 0;
  }
  .list-items {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-1);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .list-item {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-2);
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-xs);
    text-align: left;
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out);
  }
  .list-item:hover {
    background: var(--color-hover);
  }
  .list-item.active {
    background: color-mix(in srgb, var(--color-primary) 14%, transparent);
    color: var(--color-primary);
  }
  .list-item-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    flex-shrink: 0;
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 0 6px;
    border-radius: var(--radius-sm);
  }
  .badge.builtin {
    background: color-mix(in srgb, var(--color-primary) 18%, transparent);
    color: var(--color-primary);
  }
  .badge.disabled {
    background: var(--color-error-bg);
    color: var(--color-error);
  }
  .list-empty {
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    padding: var(--space-4) var(--space-2);
  }

  /* 右侧表单 */
  .provider-form {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: 2px;
  }
  .form-section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    padding: var(--space-3);
  }
  .form-section-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--color-text);
  }
  .form-hint {
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    margin: 0;
  }

  .field-row {
    display: flex;
    gap: var(--space-3);
    flex-wrap: wrap;
    align-items: flex-end;
  }
  .field {
    flex: 1;
    min-width: 150px;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--fs-xs);
  }
  .field-narrow {
    flex: 0 1 130px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--fs-xs);
  }
  .field-label {
    color: var(--color-text-muted);
  }
  .field input {
    font-size: var(--fs-sm);
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    border: var(--border-width) solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-text);
    width: 100%;
    box-sizing: border-box;
    transition: border-color var(--duration-fast) var(--ease-out), box-shadow var(--duration-fast) var(--ease-out);
  }
  .field-narrow input {
    font-size: var(--fs-sm);
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    border: var(--border-width) solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-text);
    width: 100%;
    box-sizing: border-box;
    transition: border-color var(--duration-fast) var(--ease-out), box-shadow var(--duration-fast) var(--ease-out);
  }
  .field input::placeholder,
  .field-narrow input::placeholder {
    color: var(--color-text-muted);
    opacity: 0.7;
  }
  .field input:focus,
  .field-narrow input:focus {
    outline: none;
    border-color: var(--color-primary);
    box-shadow: 0 0 0 3px color-mix(in oklch, var(--color-primary) 15%, transparent);
  }
  .field input:disabled {
    opacity: 0.5;
  }
  .field-toggle {
    flex: 0 0 auto;
    display: flex;
    flex-direction: row;
    align-items: center;
    white-space: nowrap;
    padding-bottom: 4px;
  }

  .model-item {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
  }
  .caps-group {
    flex: 1;
    min-width: 200px;
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
    font-size: var(--fs-xs);
    padding-bottom: 4px;
  }
  .cap-check {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--color-text);
    cursor: pointer;
    white-space: nowrap;
  }
  .cap-check input {
    accent-color: var(--color-primary);
  }
  .model-actions {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    padding-bottom: 2px;
  }

  .manager-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    flex-shrink: 0;
    gap: var(--space-2);
    padding-top: var(--space-3);
    border-top: var(--border-width) solid var(--color-border);
  }

  .btn {
    font-size: var(--fs-sm);
    padding: 5px var(--space-3);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text);
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out), border-color var(--duration-fast) var(--ease-out);
  }
  .btn:hover {
    background: var(--color-hover);
  }
  .btn.primary {
    background: var(--color-primary);
    border-color: var(--color-primary);
    color: var(--color-on-primary);
  }
  .btn.primary:hover {
    background: var(--color-primary-dim);
    border-color: var(--color-primary-dim);
  }
  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .btn.danger-outline {
    color: var(--color-error);
    border-color: color-mix(in srgb, var(--color-error) 40%, transparent);
    align-self: flex-start;
  }
  .btn.danger-outline:hover {
    background: var(--color-error-bg);
  }
</style>

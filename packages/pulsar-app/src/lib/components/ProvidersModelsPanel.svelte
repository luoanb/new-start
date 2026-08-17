<script lang="ts">
  import { t, tMap } from "$lib/i18n";
  import { useViewContext } from "$lib/layout/viewContext";
  import type { ModelInfo, ProviderInfo } from "$lib/types";

  const ctx = useViewContext();
  const data = ctx.stores.data;

  let providers = $derived(ctx.stores.data.state.providers);
  let models = $derived(ctx.stores.data.state.models);

  // 折叠的服务商 id（默认全展开）
  let collapsed = $state<Set<string>>(new Set());
  // 删除二次确认中的服务商 id
  let deleteConfirmId = $state<string | null>(null);

  function toggleCollapse(id: string) {
    const next = new Set(collapsed);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    collapsed = next;
  }

  function modelsOf(provider: ProviderInfo): ModelInfo[] {
    return models.filter((m) => m.provider_id === provider.id);
  }

  function modelCaps(m: ModelInfo): string[] {
    const caps: string[] = [];
    for (const key of Object.keys(m.capabilities)) {
      const label = tMap("sidePanel.caps", key);
      if (
        label !== `sidePanel.caps.${key}` &&
        (m.capabilities as unknown as Record<string, boolean | undefined>)[key]
      ) {
        caps.push(label);
      }
    }
    return caps;
  }
</script>

<div class="providers-models-panel">
  <div class="toolbar">
    <span class="toolbar-title">{t("sidePanel.models")}</span>
    <button
      class="btn btn-sm btn-primary"
      onclick={() => data.requestCreateProvider()}
    >
      ＋ {t("providersModelsPanel.create")}
    </button>
  </div>

  {#if providers.length === 0}
    <p class="empty">{t("sidePanel.noProviders")}</p>
  {:else}
    <div class="list">
      {#each providers as p}
        <div class="provider-group">
          <div class="provider-row" role="button" tabindex="0" onclick={() => toggleCollapse(p.id)} onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggleCollapse(p.id); } }}>
            <svg
              class="chevron"
              class:open={!collapsed.has(p.id)}
              aria-hidden="true"
              viewBox="0 0 24 24"
              width="12"
              height="12"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <polyline points="6 9 12 15 18 9" />
            </svg>
            <span class="provider-name" title={p.display_name}>{p.display_name}</span>
            <span class="mono">{p.id}</span>
            <span class="model-count">{modelsOf(p).length}</span>
            <span class="row-actions">
              <span
                class="row-btn"
                role="button"
                tabindex="0"
                onclick={(e) => {
                  e.stopPropagation();
                  data.requestEditProvider(p.id);
                }}
                onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); e.stopPropagation(); data.requestEditProvider(p.id); } }}
              >
                <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M17 3a2.83 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
                </svg>
              </span>
              {#if deleteConfirmId === p.id}
                <span class="delete-confirm" title={t("providersModelsPanel.deleteConfirm")}>
                  <button
                    class="btn btn-sm btn-danger"
                    onclick={(e) => { e.stopPropagation(); deleteConfirmId = null; data.requestEditProvider(p.id); }}
                  >
                    {t("providersModelsPanel.deleteGo")}
                  </button>
                  <button class="btn btn-sm" onclick={(e) => { e.stopPropagation(); deleteConfirmId = null; }}>
                    {t("providersModelsPanel.cancel")}
                  </button>
                </span>
              {:else}
                <span
                  class="row-btn danger"
                  role="button"
                  tabindex="0"
                  onclick={(e) => {
                    e.stopPropagation();
                    deleteConfirmId = p.id;
                  }}
                  onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); e.stopPropagation(); deleteConfirmId = p.id; } }}
                >
                  <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="3 6 5 6 21 6" />
                    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                  </svg>
                </span>
              {/if}
            </span>
          </div>

          {#if !collapsed.has(p.id)}
            <div class="models">
              {#if modelsOf(p).length === 0}
                <p class="no-models">{t("sidePanel.noModels")}</p>
              {:else}
                {#each modelsOf(p) as m}
                  <div class="model-row">
                    <span class="model-dot" aria-hidden="true"></span>
                    <span class="model-name" title={m.display_name}>{m.display_name}</span>
                    <span class="caps">
                      {#each modelCaps(m) as cap}
                        <span class="cap-tag">{cap}</span>
                      {/each}
                    </span>
                  </div>
                {/each}
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .providers-models-panel {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: var(--space-2);
    padding: var(--space-2);
    overflow: hidden;
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    flex-shrink: 0;
  }
  .toolbar-title {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--color-text);
    flex: 1;
    min-width: 0;
  }

  .btn {
    font-size: var(--fs-xs);
    padding: 4px var(--space-2);
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
  .btn-primary {
    background: var(--color-primary);
    border-color: var(--color-primary);
    color: var(--color-on-primary);
  }
  .btn-primary:hover {
    background: var(--color-primary-dim);
    border-color: var(--color-primary-dim);
  }
  .btn-danger {
    background: var(--color-error);
    border-color: var(--color-error);
    color: var(--color-on-primary, #fff);
  }
  .btn-danger:hover {
    opacity: 0.9;
  }

  .empty {
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    padding: var(--space-6) 0;
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .provider-group {
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    overflow: hidden;
  }
  .provider-row {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-2);
    cursor: pointer;
  }
  .provider-row:hover {
    background: var(--color-hover);
  }
  .chevron {
    flex-shrink: 0;
    transition: transform var(--duration-fast) var(--ease-out);
    color: var(--color-text-muted);
  }
  .chevron.open {
    transform: rotate(180deg);
  }
  .provider-name {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--color-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mono {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 90px;
  }
  .model-count {
    flex-shrink: 0;
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 0 6px;
    border-radius: var(--radius-sm);
    background: var(--color-border);
    color: var(--color-text-muted);
  }
  .row-actions {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-left: auto;
    transition: opacity var(--duration-fast) var(--ease-out);
  }
  /* 仅支持 hover 的设备隐藏行操作按钮（hover/键盘聚焦时显示）；
     触屏（hover: none）始终可见，保证可发现性。见 .cursor/rules/ui-hover-reveal.mdc */
  @media (hover: hover) {
    .row-actions {
      opacity: 0;
      visibility: hidden;
    }
    .provider-row:hover .row-actions,
    .provider-row:focus-within .row-actions {
      opacity: 1;
      visibility: visible;
    }
  }
  .row-btn {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: var(--fs-xs);
    color: var(--color-primary);
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
  }
  .row-btn:hover {
    text-decoration: underline;
  }
  .row-btn.danger {
    color: var(--color-error);
  }
  .delete-confirm {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .models {
    display: flex;
    flex-direction: column;
    border-top: var(--border-width) solid var(--color-border);
  }
  .model-row {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-3);
    font-size: var(--fs-xs);
  }
  .model-row:hover {
    background: var(--color-hover);
  }
  .model-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-text-muted);
    opacity: 0.5;
    flex-shrink: 0;
  }
  .model-name {
    font-size: var(--fs-sm);
    font-weight: 500;
    color: var(--color-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }
  .caps {
    display: none;
    flex-wrap: wrap;
    gap: 3px;
    margin-left: auto;
    justify-content: flex-end;
  }
  .model-row:hover .caps {
    display: flex;
  }
  .cap-tag {
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--color-primary);
    color: var(--color-on-primary);
    opacity: 0.85;
  }
  .no-models {
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    padding: var(--space-2);
    margin: 0;
  }
</style>

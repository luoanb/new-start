<script lang="ts">
  import type { ProviderInfo, ModelInfo, SkillInfo } from "$lib/types";
  import { t, tMap } from "$lib/i18n";

  let {
    providers,
    models,
    skills,
  }: {
    providers: ProviderInfo[];
    models: ModelInfo[];
    skills: SkillInfo[];
  } = $props();

  let activeTab = $state("providers");

  type Tab = { id: string; label: string };
  let tabs: Tab[] = $derived([
    { id: "providers", label: t("sidePanel.providers") },
    { id: "models", label: t("sidePanel.models") },
    { id: "skills", label: t("sidePanel.skills") },
  ]);

  function modelCaps(m: ModelInfo): string[] {
    const caps: string[] = [];
    for (const key of Object.keys(m.capabilities)) {
      const label = tMap("sidePanel.caps", key);
      if (label !== `sidePanel.caps.${key}` && (m.capabilities as unknown as Record<string, boolean | undefined>)[key]) {
        caps.push(label);
      }
    }
    return caps;
  }

  function formatPrice(val: number | undefined, suffix: string): string {
    if (val == null) return "-";
    return `$${val.toFixed(2)}/${suffix}`;
  }
</script>

<div class="side-panel">
  <div class="tabs">
    {#each tabs as tab}
      <button
        class="tab"
        class:active={activeTab === tab.id}
        onclick={() => (activeTab = tab.id)}
      >
        {tab.label}
      </button>
    {/each}
  </div>

  <div class="tab-content">
    {#if activeTab === "providers"}
      {#if providers.length === 0}
        <p class="empty">{t("sidePanel.noProviders")}</p>
      {:else}
        <div class="list">
          {#each providers as p}
            <div class="item">
              <div class="item-title">{p.display_name}</div>
              <div class="item-detail">{t("sidePanel.id")}: {p.id}</div>
              <div class="item-detail">{t("sidePanel.auth")}: {p.auth_env}</div>
              {#if p.api_base}
                <div class="item-detail">{t("sidePanel.api")}: {p.api_base}</div>
              {/if}
              <div class="item-detail">{t("sidePanel.kind")}: {p.kind}</div>
            </div>
          {/each}
        </div>
      {/if}
    {:else if activeTab === "models"}
      {#if models.length === 0}
        <p class="empty">{t("sidePanel.noModels")}</p>
      {:else}
        <div class="list">
          {#each models as m}
            <div class="item">
              <div class="item-title">{m.display_name}</div>
              <div class="item-detail">{m.id} ({m.provider_id})</div>
              <div class="caps">
                {#each modelCaps(m) as cap}
                  <span class="cap-tag">{cap}</span>
                {/each}
              </div>
              <div class="item-detail">
                {t("sidePanel.context")}: {m.context_window?.toLocaleString() ?? "-"} {t("sidePanel.tokens")} &middot;
                {t("sidePanel.output")}: {m.max_output_tokens?.toLocaleString() ?? "-"} {t("sidePanel.tokens")}
              </div>
              <div class="item-detail">
                {formatPrice(m.pricing_input, t("sidePanel.mIn"))} &middot;
                {formatPrice(m.pricing_output, t("sidePanel.mOut"))}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {:else if activeTab === "skills"}
      {#if skills.length === 0}
        <p class="empty">{t("sidePanel.noSkills")}</p>
      {:else}
        <div class="list">
          {#each skills as s}
            <div class="item">
              <div class="item-title">{s.name}</div>
              <div class="item-detail">{s.description}</div>
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .side-panel { display: flex; flex-direction: column; height: 100%; }
  .tabs { display: flex; border-bottom: var(--border-width) solid var(--color-border); }
  .tab { flex: 1; padding: var(--space-2); border: none; background: transparent; cursor: pointer; font-size: var(--fs-sm); font-weight: 500; color: var(--color-text-muted); transition: color var(--duration-fast) var(--ease-out), border-color var(--duration-fast) var(--ease-out); border-bottom: 2px solid transparent; }
  .tab.active { color: var(--color-primary); border-bottom-color: var(--color-primary); }
  .tab:hover { color: var(--color-text); }
  .tab-content { flex: 1; overflow-y: auto; padding: var(--space-2); }
  .empty { text-align: center; color: var(--color-text-muted); font-size: var(--fs-sm); padding: var(--space-6) var(--space-2); }
  .list { display: flex; flex-direction: column; gap: var(--space-1); }
  .item { padding: var(--space-2) var(--space-2); border-radius: var(--radius-md); background: var(--color-bg); border: var(--border-width) solid var(--color-border); }
  .item-title { font-size: var(--fs-sm); font-weight: 600; margin-bottom: var(--space-1); }
  .item-detail { font-size: var(--fs-xs); color: var(--color-text-muted); line-height: 1.5; }
  .caps { display: flex; flex-wrap: wrap; gap: var(--space-1); margin: var(--space-1) 0; }
  .cap-tag { font-size: 10px; font-weight: 600; padding: 1px 6px; border-radius: var(--radius-sm); background: var(--color-primary); color: var(--color-on-primary); opacity: 0.85; }
</style>

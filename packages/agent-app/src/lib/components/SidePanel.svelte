<script lang="ts">
  import type { ProviderInfo, ModelInfo, SkillInfo } from "$lib/types";

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
  const tabs: Tab[] = [
    { id: "providers", label: "Providers" },
    { id: "models", label: "Models" },
    { id: "skills", label: "Skills" },
  ];

  const capLabels: Record<string, string> = {
    chat: "Chat",
    tools: "Tools",
    streaming: "Stream",
    structured_output: "JSON",
    vision: "Vision",
  };

  function modelCaps(m: ModelInfo): string[] {
    const caps: string[] = [];
    for (const [key, label] of Object.entries(capLabels)) {
      if ((m.capabilities as unknown as Record<string, boolean | undefined>)[key]) {
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
        <p class="empty">No providers configured.</p>
      {:else}
        <div class="list">
          {#each providers as p}
            <div class="item">
              <div class="item-title">{p.display_name}</div>
              <div class="item-detail">ID: {p.id}</div>
              <div class="item-detail">Auth: {p.auth_env}</div>
              {#if p.api_base}
                <div class="item-detail">API: {p.api_base}</div>
              {/if}
              <div class="item-detail">Kind: {p.kind}</div>
            </div>
          {/each}
        </div>
      {/if}
    {:else if activeTab === "models"}
      {#if models.length === 0}
        <p class="empty">No models available.</p>
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
                Context: {m.context_window?.toLocaleString() ?? "-"} tokens
                &middot;
                Output: {m.max_output_tokens?.toLocaleString() ?? "-"} tokens
              </div>
              <div class="item-detail">
                {formatPrice(m.pricing_input, "M in")} &middot;
                {formatPrice(m.pricing_output, "M out")}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {:else if activeTab === "skills"}
      {#if skills.length === 0}
        <p class="empty">No skills available.</p>
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
  .side-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .tabs {
    display: flex;
    border-bottom: 1px solid var(--color-border);
  }

  .tab {
    flex: 1;
    padding: 8px;
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-muted);
    transition: color 0.15s, border-color 0.15s;
    border-bottom: 2px solid transparent;
  }

  .tab.active {
    color: var(--color-primary);
    border-bottom-color: var(--color-primary);
  }

  .tab:hover {
    color: var(--color-text);
  }

  .tab-content {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
  }

  .empty {
    text-align: center;
    color: var(--color-text-muted);
    font-size: 13px;
    padding: 24px 8px;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .item {
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--color-bg);
    border: 1px solid var(--color-border);
  }

  .item-title {
    font-size: 13px;
    font-weight: 600;
    margin-bottom: 4px;
  }

  .item-detail {
    font-size: 11px;
    color: var(--color-text-muted);
    line-height: 1.5;
  }

  .caps {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 4px 0;
  }

  .cap-tag {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--color-primary);
    color: var(--color-on-primary);
    opacity: 0.85;
  }
</style>

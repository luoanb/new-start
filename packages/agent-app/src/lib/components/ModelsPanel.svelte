<script lang="ts">
  import type { ModelInfo } from "$lib/types";
  import { t, tMap } from "$lib/i18n";
  import { useViewContext } from "$lib/layout/viewContext";

  // 数据来自 ViewContext（容器与内容解耦，无 props）。
  const ctx = useViewContext();
  let models = $derived(ctx.stores.data.state.models);

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

<div class="models-panel">
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
</div>

<style>
  .models-panel { height: 100%; overflow-y: auto; padding: var(--space-2); }
  .empty { text-align: center; color: var(--color-text-muted); font-size: var(--fs-sm); padding: var(--space-6) var(--space-2); }
  .list { display: flex; flex-direction: column; gap: var(--space-1); }
  .item { padding: var(--space-2); border-radius: var(--radius-md); background: var(--color-bg); border: var(--border-width) solid var(--color-border); }
  .item-title { font-size: var(--fs-sm); font-weight: 600; margin-bottom: var(--space-1); }
  .item-detail { font-size: var(--fs-xs); color: var(--color-text-muted); line-height: 1.5; }
  .caps { display: flex; flex-wrap: wrap; gap: var(--space-1); margin: var(--space-1) 0; }
  .cap-tag { font-size: 10px; font-weight: 600; padding: 1px 6px; border-radius: var(--radius-sm); background: var(--color-primary); color: var(--color-on-primary); opacity: 0.85; }
</style>

<script lang="ts">
  import { t } from "$lib/i18n";
  import { useViewContext } from "$lib/layout/viewContext";

  // 数据来自 ViewContext（容器与内容解耦，无 props）。
  const ctx = useViewContext();
  let providers = $derived(ctx.stores.data.state.providers);
</script>

<div class="providers-panel">
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
</div>

<style>
  .providers-panel { height: 100%; overflow-y: auto; padding: var(--space-2); }
  .empty { text-align: center; color: var(--color-text-muted); font-size: var(--fs-sm); padding: var(--space-6) var(--space-2); }
  .list { display: flex; flex-direction: column; gap: var(--space-1); }
  .item { padding: var(--space-2); border-radius: var(--radius-md); background: var(--color-bg); border: var(--border-width) solid var(--color-border); }
  .item-title { font-size: var(--fs-sm); font-weight: 600; margin-bottom: var(--space-1); }
  .item-detail { font-size: var(--fs-xs); color: var(--color-text-muted); line-height: 1.5; }
</style>

<script lang="ts">
  import { useViewContext } from "$lib/layout/viewContext";
  import { t } from "$lib/i18n";

  // 示例视图：演示「容器与内容解耦」——视图不接收 props，
  // 数据与命令一律经 useViewContext() 自取。
  const ctx = useViewContext();
  const data = ctx.stores.data;
  const layout = ctx.stores.layout;

  function openPanel() {
    layout.insertPanel("example");
  }
</script>

<div class="example-view">
  <h2 class="title">{t("views.example")}</h2>
  <p class="hint">{t("views.exampleHint")}</p>

  <ul class="facts">
    <li>
      <span class="key">{t("views.exampleReady")}</span>
      <span class="val">{data.ready ? "true" : "false"}</span>
    </li>
    <li>
      <span class="key">{t("views.lastEvent")}</span>
      <span class="val">{data.lastKind ?? "—"}</span>
    </li>
  </ul>

  <div class="actions">
    <button type="button" onclick={openPanel}>{t("views.exampleOpenPanel")}</button>
    <button type="button" onclick={() => ctx.commands.showError(t("views.exampleError"))}>
      {t("views.exampleTriggerError")}
    </button>
  </div>
</div>

<style>
  .example-view {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    color: var(--color-text);
    overflow: auto;
  }

  .title {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 600;
  }

  .hint {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--color-text-muted);
  }

  .facts {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--fs-sm);
  }

  .facts li {
    display: flex;
    gap: var(--space-2);
  }

  .key {
    color: var(--color-text-muted);
    min-width: 96px;
  }

  .val {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  }

  .actions {
    display: flex;
    gap: var(--space-2);
  }

  .actions button {
    font-size: var(--fs-xs);
    padding: 4px 6px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
  }

  .actions button:hover {
    border-color: var(--color-primary);
    color: var(--color-primary);
  }
</style>

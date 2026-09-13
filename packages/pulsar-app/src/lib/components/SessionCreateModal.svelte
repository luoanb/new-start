<script lang="ts">
  import { t } from "$lib/i18n";
  import { SESSION_MODES } from "$lib/sessionModes";

  let { open, onCreate, onClose }: {
    open: boolean;
    onCreate: (mode: string) => void;
    onClose: () => void;
  } = $props();

  // 模式清单来自 $lib/sessionModes：与会话面板组合按钮/顶栏新建入口共用同一份定义。
  const modes = SESSION_MODES;
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="overlay" onclick={onClose}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <h2>{t("createModal.title")}</h2>
        <button class="close-btn" onclick={onClose}>×</button>
      </div>
      <div class="modal-body">
        <p class="hint">{t("createModal.hint")}</p>
        <div class="mode-options">
          {#each modes as mode (mode.id)}
            <button class="mode-card" onclick={() => onCreate(mode.id)}>
              <strong>{t(mode.labelKey)}</strong>
              <span class="mode-desc">{t(mode.descKey)}</span>
            </button>
          {/each}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.4); display: flex; align-items: center; justify-content: center; z-index: 100; }
  .modal { background: var(--color-surface); border-radius: 16px; width: 380px; max-width: 90vw; box-shadow: 0 8px 32px rgba(0,0,0,0.2); }
  .modal-header { display: flex; align-items: center; justify-content: space-between; padding: 16px 20px; border-bottom: 1px solid var(--color-border); }
  .modal-header h2 { margin: 0; font-size: var(--fs-lg); font-weight: 600; }
  .close-btn { background: none; border: none; font-size: 22px; cursor: pointer; color: var(--color-text); padding: 0 4px; line-height: 1; }
  .modal-body { padding: 20px; }
  .hint { margin: 0 0 16px; font-size: var(--fs-base); color: var(--color-text-muted); }
  .mode-options { display: flex; flex-direction: column; gap: 8px; }
  .mode-card { display: flex; flex-direction: column; gap: 4px; padding: 12px 16px; border: 1px solid var(--color-border); border-radius: 10px; background: var(--color-bg); cursor: pointer; text-align: left; transition: border-color 0.15s, background 0.15s; color: var(--color-text); }
  .mode-card:hover { border-color: var(--color-primary); background: var(--color-hover); }
  .mode-desc { font-size: var(--fs-sm); color: var(--color-text-muted); }
</style>

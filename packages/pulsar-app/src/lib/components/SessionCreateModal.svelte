<script lang="ts">
  import { t } from "$lib/i18n";

  let { open, onCreate, onClose }: {
    open: boolean;
    onCreate: (mode: string) => void;
    onClose: () => void;
  } = $props();

  const modes = [
    { id: "chat", label: () => t("createModal.chatLabel"), desc: () => t("createModal.chatDesc") },
    { id: "assistant", label: () => t("createModal.assistantLabel"), desc: () => t("createModal.assistantDesc") },
    // Agent 模式暂不提供新建入口（隐藏，后端与历史会话保留）。
    { id: "system", label: () => t("createModal.systemLabel"), desc: () => t("createModal.systemDesc") },
  ];
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
          {#each modes as mode}
            <button class="mode-card" onclick={() => onCreate(mode.id)}>
              <strong>{mode.label()}</strong>
              <span class="mode-desc">{mode.desc()}</span>
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

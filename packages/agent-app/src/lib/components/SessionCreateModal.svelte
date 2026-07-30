<script lang="ts">
  import { locale, t } from "$lib/i18n";
  $locale;

  let { open, onCreate, onClose }: {
    open: boolean;
    onCreate: (mode: string) => void;
    onClose: () => void;
  } = $props();

  let modes = $derived([
    { id: "chat", label: t("createModal.chatLabel"), desc: t("createModal.chatDesc") },
    { id: "agent", label: t("createModal.agentLabel"), desc: t("createModal.agentDesc") },
    { id: "assistant", label: t("createModal.assistantLabel"), desc: t("createModal.assistantDesc") },
  ]);

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="overlay" onclick={onClose} onkeydown={handleKeydown}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="modal" onclick={(e) => e.stopPropagation()} onkeydown={handleKeydown}>
      <div class="modal-header">
        <h2>{t("createModal.title")}</h2>
        <button class="close-btn" onclick={onClose}>×</button>
      </div>
      <div class="modal-body">
        <p class="hint">{t("createModal.hint")}</p>
        <div class="mode-options">
          {#each modes as mode}
            <button class="mode-card" onclick={() => onCreate(mode.id)}>
              <strong>{mode.label}</strong>
              <span class="mode-desc">{mode.desc}</span>
            </button>
          {/each}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.4); display: flex; align-items: center; justify-content: center; z-index: 100; }
  .modal { background: var(--color-surface); border-radius: var(--radius-lg); width: 380px; max-width: 90vw; box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2); }
  .modal-header { display: flex; align-items: center; justify-content: space-between; padding: var(--space-4) var(--space-5); border-bottom: var(--border-width) solid var(--color-border); }
  .modal-header h2 { margin: 0; font-size: var(--fs-xl); font-weight: 600; }
  .close-btn { background: none; border: none; font-size: 22px; cursor: pointer; color: var(--color-text); padding: 0 4px; line-height: 1; }
  .modal-body { padding: var(--space-5); }
  .hint { margin: 0 0 var(--space-4); font-size: var(--fs-base); color: var(--color-text-muted); }
  .mode-options { display: flex; flex-direction: column; gap: var(--space-2); }
  .mode-card { display: flex; flex-direction: column; gap: var(--space-1); padding: var(--space-3) var(--space-4); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-md); background: var(--color-bg); cursor: pointer; text-align: left; transition: border-color var(--duration-fast) var(--ease-out), background var(--duration-fast) var(--ease-out); color: var(--color-text); }
  .mode-card:hover { border-color: var(--color-primary); background: var(--color-hover); }
  .mode-desc { font-size: var(--fs-sm); color: var(--color-text-muted); }
</style>

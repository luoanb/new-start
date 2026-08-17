<script lang="ts">
  import { t } from "$lib/i18n";
  import ThemeSwitcher from "./ThemeSwitcher.svelte";
  import LocaleSwitcher from "./LocaleSwitcher.svelte";

  let { open, onClose }: { open: boolean; onClose: () => void } = $props();
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="overlay" onclick={onClose}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <h2>{t("settings.title")}</h2>
        <button class="close-btn" onclick={onClose}>×</button>
      </div>
      <div class="modal-body">
        <div class="field">
          <span class="field-label">{t("settings.theme")}</span>
          <ThemeSwitcher />
        </div>
        <div class="field">
          <span class="field-label">{t("settings.language")}</span>
          <LocaleSwitcher />
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed; inset: 0; background: rgba(0, 0, 0, 0.4);
    display: flex; align-items: center; justify-content: center; z-index: 100;
  }
  .modal {
    background: var(--color-surface); border-radius: 16px; width: 360px; max-width: 90vw;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
  }
  .modal-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 16px 20px; border-bottom: 1px solid var(--color-border);
  }
  .modal-header h2 { margin: 0; font-size: var(--fs-lg); font-weight: 600; }
  .close-btn {
    background: none; border: none; font-size: 22px; cursor: pointer;
    color: var(--color-text); padding: 0 4px; line-height: 1;
  }
  .modal-body {
    padding: 20px; display: flex; flex-direction: column; gap: 16px;
  }
  .field { display: flex; flex-direction: column; gap: 8px; }
  .field-label { font-size: var(--fs-sm); color: var(--color-text-muted); }
</style>

<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";

  // 轻量确认弹窗：对齐 ConnectDialog 浮层词汇（overlay + surface + radius-lg 容器）。
  // danger=true 时确认按钮走 .btn-danger（删除/覆盖等破坏性操作）。
  let {
    open,
    title,
    message,
    confirmLabel,
    cancelLabel,
    danger = false,
    onConfirm,
    onCancel,
  }: {
    open: boolean;
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    } else if (e.key === "Enter") {
      e.preventDefault();
      onConfirm();
    }
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="overlay" role="presentation" onclick={onCancel}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <h2>{title}</h2>
      </div>
      <div class="modal-body">
        <p class="message">{message}</p>
      </div>
      <div class="modal-footer">
        <button class="btn" onclick={onCancel}>{cancelLabel ?? t("common.cancel")}</button>
        <button class={danger ? "btn btn-danger" : "btn btn-primary"} onclick={onConfirm}>
          {confirmLabel ?? t("common.confirm")}
        </button>
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
    background: var(--color-surface); border-radius: 16px; width: 400px; max-width: 90vw;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
  }
  .modal-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 16px 20px; border-bottom: 1px solid var(--color-border);
  }
  .modal-header h2 { margin: 0; font-size: 16px; font-weight: 600; }
  .modal-body { padding: 20px; }
  .message { font-size: 14px; color: var(--color-text-muted); line-height: 1.6; }
  .modal-footer {
    display: flex; justify-content: flex-end; gap: 8px;
    padding: 12px 20px; border-top: 1px solid var(--color-border);
  }
</style>

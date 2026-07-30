<script lang="ts">
  import { locale, t } from "$lib/i18n";
  $locale;

  let { onSend, loading = false }: { onSend: (text: string) => void; loading?: boolean } = $props();

  let text = $state("");
  let composing = $state(false);
  let history: string[] = $state([]);
  let historyIndex = $state(-1);
  let textareaEl: HTMLTextAreaElement | undefined = $state();

  function handleCompositionStart() { composing = true; }
  function handleCompositionEnd() { composing = false; }

  function handleKeydown(e: KeyboardEvent) {
    if (composing || e.isComposing || e.key === "Process" || e.key === "Dead") return;
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); submit(); }
    else if (e.key === "ArrowUp" && history.length > 0) {
      e.preventDefault();
      historyIndex = historyIndex === -1 ? history.length - 1 : Math.max(0, historyIndex - 1);
      text = history[historyIndex];
    } else if (e.key === "ArrowDown" && historyIndex !== -1) {
      e.preventDefault();
      if (historyIndex < history.length - 1) { historyIndex++; text = history[historyIndex]; }
      else { historyIndex = -1; text = ""; }
    }
  }

  function submit() {
    const trimmed = text.trim();
    if (!trimmed || loading) return;
    history.push(trimmed);
    historyIndex = -1;
    onSend(trimmed);
    text = "";
    setTimeout(() => textareaEl?.focus(), 0);
  }
</script>

<div class="input-area">
  <textarea
    bind:this={textareaEl}
    bind:value={text}
    onkeydown={handleKeydown}
    oncompositionstart={handleCompositionStart}
    oncompositionend={handleCompositionEnd}
    placeholder={t("chatArea.chatInputPlaceholder")}
    disabled={loading}
    rows="1"
  ></textarea>
  <button onclick={submit} disabled={loading || !text.trim()}>
    {loading ? t("common.sending") : t("common.send")}
  </button>
</div>

<style>
  .input-area { display: flex; gap: var(--space-2); padding: var(--space-3) var(--space-4); border-top: var(--border-width) solid var(--color-border); background: var(--color-surface); }
  textarea { flex: 1; resize: none; border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); padding: var(--space-2) var(--space-3); font-size: var(--fs-base); font-family: inherit; line-height: 1.4; background: var(--color-bg); color: var(--color-text); outline: none; max-height: 120px; transition: border-color var(--duration-fast) var(--ease-out); }
  textarea:focus { border-color: var(--color-primary); }
  textarea:disabled { opacity: 0.5; }
  button { align-self: flex-end; padding: var(--space-2) var(--space-5); border-radius: var(--radius-sm); border: none; background: var(--color-primary); color: var(--color-on-primary); font-size: var(--fs-base); font-weight: 500; cursor: pointer; transition: opacity var(--duration-fast) var(--ease-out); white-space: nowrap; }
  button:disabled { opacity: 0.4; cursor: not-allowed; }
  button:not(:disabled):hover { opacity: 0.9; }
</style>

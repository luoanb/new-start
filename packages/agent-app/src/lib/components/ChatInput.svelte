<script lang="ts">
  let { onSend, loading = false }: { onSend: (text: string) => void; loading?: boolean } = $props();

  let text = $state("");
  let composing = $state(false);
  let history: string[] = $state([]);
  let historyIndex = $state(-1);
  let textareaEl: HTMLTextAreaElement | undefined = $state();

  function handleCompositionStart() { composing = true; }
  function handleCompositionEnd() { composing = false; }

  function handleKeydown(e: KeyboardEvent) {
    // Ignore events during IME composition (e.g. Chinese input)
    if (composing || e.isComposing || e.key === "Process" || e.key === "Dead") return;

    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    } else if (e.key === "ArrowUp" && history.length > 0) {
      e.preventDefault();
      if (historyIndex === -1) {
        historyIndex = history.length - 1;
      } else if (historyIndex > 0) {
        historyIndex--;
      }
      text = history[historyIndex];
    } else if (e.key === "ArrowDown" && historyIndex !== -1) {
      e.preventDefault();
      if (historyIndex < history.length - 1) {
        historyIndex++;
        text = history[historyIndex];
      } else {
        historyIndex = -1;
        text = "";
      }
    }
  }

  function submit() {
    const trimmed = text.trim();
    if (!trimmed || loading) return;
    history.push(trimmed);
    historyIndex = -1;
    onSend(trimmed);
    text = "";

    // Refocus the textarea after sending
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
    placeholder="Type a message... (Enter to send, Shift+Enter for new line)"
    disabled={loading}
    rows="1"
  ></textarea>
  <button onclick={submit} disabled={loading || !text.trim()}>
    {loading ? "Sending..." : "Send"}
  </button>
</div>

<style>
  .input-area {
    display: flex;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  textarea {
    flex: 1;
    resize: none;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 10px 12px;
    font-size: 14px;
    font-family: inherit;
    line-height: 1.4;
    background: var(--color-bg);
    color: var(--color-text);
    outline: none;
    max-height: 120px;
    transition: border-color 0.15s;
  }

  textarea:focus {
    border-color: var(--color-primary);
  }

  textarea:disabled {
    opacity: 0.5;
  }

  button {
    align-self: flex-end;
    padding: 10px 20px;
    border-radius: 8px;
    border: none;
    background: var(--color-primary);
    color: var(--color-on-primary);
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    transition: opacity 0.15s;
    white-space: nowrap;
  }

  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  button:not(:disabled):hover {
    opacity: 0.9;
  }
</style>

<script lang="ts">
  import ModelPicker from "./ModelPicker.svelte";
  import type { ProviderInfo, ModelInfo, SamplingParams, ThinkingConfig } from "$lib/types";
  import { t } from "$lib/i18n";

  let {
    onSend,
    loading = false,
    providers = [],
    models = [],
    selectedProviderId = "",
    selectedModelId = "",
    params,
    thinking,
    onModelChange,
  }: {
    onSend: (text: string) => void;
    loading?: boolean;
    providers?: ProviderInfo[];
    models?: ModelInfo[];
    selectedProviderId?: string;
    selectedModelId?: string;
    params?: SamplingParams;
    thinking?: ThinkingConfig;
    onModelChange?: (providerId: string, modelId: string, params?: SamplingParams, thinking?: ThinkingConfig) => void;
  } = $props();

  let text = $state("");
  let composing = $state(false);
  let history: string[] = $state([]);
  let historyIndex = $state(-1);
  let textareaEl: HTMLTextAreaElement | undefined = $state();

  const MAX_HEIGHT = 218; // 约 10 行（10 × 21px 行高 + 上下内边距 8px），之后滚动

  function handleCompositionStart() { composing = true; }
  function handleCompositionEnd() { composing = false; }

  function autoResize() {
    const el = textareaEl;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, MAX_HEIGHT) + "px";
  }

  function handleInput() { autoResize(); }

  function handleKeydown(e: KeyboardEvent) {
    if (composing || e.isComposing || e.key === "Process" || e.key === "Dead") return;
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); submit(); }
    else if (e.key === "ArrowUp" && history.length > 0) {
      e.preventDefault();
      historyIndex = historyIndex === -1 ? history.length - 1 : Math.max(0, historyIndex - 1);
      text = history[historyIndex];
      requestAnimationFrame(autoResize);
    } else if (e.key === "ArrowDown" && historyIndex !== -1) {
      e.preventDefault();
      if (historyIndex < history.length - 1) { historyIndex++; text = history[historyIndex]; }
      else { historyIndex = -1; text = ""; }
      requestAnimationFrame(autoResize);
    }
  }

  function submit() {
    const trimmed = text.trim();
    if (!trimmed || loading) return;
    history.push(trimmed);
    historyIndex = -1;
    onSend(trimmed);
    text = "";
    requestAnimationFrame(() => {
      autoResize();
      textareaEl?.focus();
    });
  }
</script>

<div class="input-area">
  <div class="input-box">
    <textarea
      bind:this={textareaEl}
      bind:value={text}
      oninput={handleInput}
      onkeydown={handleKeydown}
      oncompositionstart={handleCompositionStart}
      oncompositionend={handleCompositionEnd}
      placeholder={t("chatArea.chatInputPlaceholder")}
      disabled={loading}
      rows="1"
    ></textarea>
    <div class="input-footer">
      <div class="footer-left">
        <ModelPicker
          {providers}
          {models}
          {selectedProviderId}
          {selectedModelId}
          {params}
          {thinking}
          onChange={onModelChange}
        />
      </div>
      <button
        class="send-btn"
        onclick={submit}
        disabled={loading || !text.trim()}
        title={t("common.send")}
        aria-label={t("common.send")}
      >
        {#if loading}
          <span class="dots">
            <span class="dot"></span>
            <span class="dot"></span>
            <span class="dot"></span>
          </span>
        {:else}
          <svg
            viewBox="0 0 24 24"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="m22 2-7 20-4-9-9-4Z" />
            <path d="M22 2 11 13" />
          </svg>
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  .input-area { display: flex; justify-content: center; padding: var(--space-1) var(--space-3) var(--space-3); background: var(--color-bg); }
  .input-box { display: flex; flex-direction: column; width: 100%; padding: var(--space-1); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-lg); background: var(--color-surface); transition: border-color var(--duration-fast) var(--ease-out), box-shadow var(--duration-fast) var(--ease-out); }
  .input-box:focus-within { border-color: var(--color-primary); box-shadow: 0 0 0 3px color-mix(in oklch, var(--color-primary) 14%, transparent); }
  textarea { width: 100%; min-height: 24px; max-height: 218px; border: none; background: transparent; resize: none; outline: none; padding: var(--space-1) var(--space-2); font-size: var(--fs-base); font-family: inherit; line-height: 1.5; color: var(--color-text); }
  textarea::placeholder { color: var(--color-text-muted); }
  textarea:disabled { opacity: 0.5; }
  .input-footer { display: flex; justify-content: space-between; align-items: center; gap: var(--space-2); padding: var(--space-1) var(--space-2); }
  .footer-left { display: flex; align-items: center; min-width: 0; }
  .send-btn { display: inline-flex; align-items: center; justify-content: center; width: 30px; height: 30px; border: none; border-radius: var(--radius-full); background: var(--color-primary); color: var(--color-on-primary); cursor: pointer; transition: background var(--duration-fast) var(--ease-out), opacity var(--duration-fast) var(--ease-out); }
  .send-btn:not(:disabled):hover { background: var(--color-primary-dim); }
  .send-btn:disabled { background: var(--color-border); color: var(--color-text-muted); cursor: not-allowed; }
  .dots { display: inline-flex; gap: 3px; }
  .dot { width: 4px; height: 4px; border-radius: var(--radius-full); background: currentColor; animation: dot-pulse 1s var(--ease-out) infinite; }
  .dot:nth-child(2) { animation-delay: 0.15s; }
  .dot:nth-child(3) { animation-delay: 0.3s; }
  @keyframes dot-pulse { 0%, 100% { opacity: 0.4; transform: scale(1); } 50% { opacity: 1; transform: scale(1.25); } }
</style>

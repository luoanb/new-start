<script lang="ts">
  import type { Message } from "$lib/types";
  import { t } from "$lib/i18n";
  import CopyButton from "./CopyButton.svelte";

  let { message }: { message: Message } = $props();

  type CmdResult = {
    exit_code?: number;
    stdout?: string;
    stderr?: string;
    timed_out?: boolean;
  };

  let expanded = $state(false);

  function toggle() {
    expanded = !expanded;
  }

  // 本组件只渲染 tool_result 消息（由 ChatMessage 按 body.kind 分发）。
  let toolName = $derived(
    message.body.kind === "tool_result" ? message.body.tool_name : ""
  );
  let content = $derived(
    message.body.kind === "tool_result" ? message.body.content : ""
  );

  let parsed = $derived.by(() => {
    try {
      const v = JSON.parse(content);
      if (v && typeof v === "object") {
        return { result: v as CmdResult };
      }
    } catch {
      // 非 JSON：按纯文本展示
    }
    return { result: null };
  });

  let label = $derived(toolName || t("toolResult.executed"));
  let result = $derived(parsed.result);
  let isCmdShape = $derived(
    result !== null &&
      ("exit_code" in result || "stdout" in result || "stderr" in result)
  );
  let isSuccess = $derived(result?.exit_code === 0);
</script>

<div class="toolresult-block" class:expanded class:cmd-shape={isCmdShape}>
  <div class="block-header">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="summary"
      role="button"
      tabindex="0"
      onclick={toggle}
      onkeydown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          toggle();
        }
      }}
    >
      <span class="label">
        <svg
          class="label-ico"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <rect width="20" height="14" x="2" y="3" rx="2" />
          <line x1="8" x2="16" y1="21" y2="21" />
          <line x1="12" x2="12" y1="17" y2="21" />
        </svg>
        <span class="label-text">{label}</span>
      </span>
      {#if isCmdShape}
        {#if result!.timed_out}
          <span class="badge timeout">{t("toolResult.timedOut")}</span>
        {/if}
        <span class="exit" class:error={!isSuccess}>{result!.exit_code}</span>
      {/if}
      <span class="block-header-actions" onclick={(e) => e.stopPropagation()}>
        <!-- 工具结果的复制：仅复制工具输出（content） -->
        <CopyButton text={content} />
      </span>
      <span class="toggle-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="m9 18 6-6-6-6" />
        </svg>
      </span>
    </div>
  </div>

  {#if expanded}
    <div class="detail">
      {#if isCmdShape}
        {#if result!.stdout}
          <div class="section">
            <span class="section-label">{t("toolResult.stdout")}</span>
            <pre class="output">{result!.stdout}</pre>
          </div>
        {/if}
        {#if result!.stderr}
          <div class="section">
            <span class="section-label">{t("toolResult.stderr")}</span>
            <pre class="output stderr">{result!.stderr}</pre>
          </div>
        {/if}
        {#if !result!.stdout && !result!.stderr}
          <p class="empty">{t("toolResult.empty")}</p>
        {/if}
      {:else}
        <pre class="output fallback">{content}</pre>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* 消息区卡片统一规范：surface 底 + 淡边框 + radius-sm，无 accent 竖条/动画。 */
  .toolresult-block { margin-top: var(--space-2); border-radius: var(--radius-sm); background: var(--color-surface); border: var(--border-width) solid var(--color-border); overflow: hidden; }
  .block-header { display: flex; }
  .summary {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    width: 100%;
    padding: var(--space-1) var(--space-2);
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-xs);
    cursor: pointer;
    text-align: left;
    border-radius: var(--radius-sm);
    transition: background var(--duration-fast) var(--ease-out);
  }
  .summary:hover {
    background: var(--color-hover);
  }
  .block-header-actions { flex-shrink: 0; display: inline-flex; align-items: center; }
  .toggle-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    color: var(--color-text-muted);
    flex-shrink: 0;
    transition: transform var(--duration-fast) var(--ease-out);
    transform-origin: center;
  }
  .toggle-icon svg {
    width: 12px;
    height: 12px;
    display: block;
  }
  .expanded .toggle-icon { transform: rotate(90deg); }
  .label {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    color: var(--color-text);
  }
  .label-ico {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    color: var(--color-text-muted);
  }
  .label-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    font-size: var(--fs-xs);
    color: var(--color-error);
    flex-shrink: 0;
  }
  .exit {
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    color: var(--color-success);
    flex-shrink: 0;
  }
  .exit.error {
    color: var(--color-error);
  }
  .detail {
    border-top: var(--border-width) solid var(--color-border);
    padding: var(--space-2);
    max-height: 400px;
    overflow-y: auto;
  }
  .section {
    margin-bottom: var(--space-2);
  }
  .section:last-child {
    margin-bottom: 0;
  }
  .section-label {
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-text-muted);
    display: block;
    margin-bottom: var(--space-1);
  }
  .output {
    margin: 0;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: oklch(0.20 0.005 75);
    color: oklch(0.88 0.004 75);
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    overflow-x: auto;
  }
  .output.stderr {
    color: oklch(0.82 0.11 25);
  }
  .output.fallback {
    margin-top: var(--space-2);
  }
  .empty {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    font-style: italic;
  }
</style>

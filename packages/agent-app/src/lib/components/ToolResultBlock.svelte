<script lang="ts">
  import type { Message } from "$lib/types";
  import { t } from "$lib/i18n";

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
  <button class="summary" onclick={toggle}>
    <span class="toggle-icon">{expanded ? "▾" : "▸"}</span>
    <span class="label">🖥 {label}</span>
    {#if isCmdShape}
      {#if result!.timed_out}
        <span class="badge timeout">{t("toolResult.timedOut")}</span>
      {/if}
      <span class="exit" class:error={!isSuccess}>{result!.exit_code}</span>
    {/if}
  </button>

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
  .toolresult-block {
    margin-top: var(--space-2);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    border: var(--border-width) solid var(--color-border);
    border-left: 3px solid var(--color-success);
    overflow: hidden;
  }
  .toolresult-block:has(.timeout) {
    border-left-color: var(--color-warning);
  }
  .summary {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-sm);
    cursor: pointer;
    text-align: left;
    transition: background var(--duration-fast) var(--ease-out);
  }
  .summary:hover {
    background: var(--color-hover);
  }
  .toggle-icon {
    font-size: 11px;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }
  .label {
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    flex: 1;
  }
  .badge {
    font-size: var(--fs-xs);
    font-family: var(--font-mono, monospace);
    padding: 1px 8px;
    border-radius: 999px;
    background: oklch(0.35 0.09 75);
    color: #fff;
  }
  .exit {
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    padding: 1px 8px;
    border-radius: 999px;
    background: oklch(0.28 0.06 150);
    color: oklch(0.85 0.08 150);
  }
  .exit.error {
    background: oklch(0.32 0.12 25);
    color: oklch(0.85 0.1 25);
  }
  .detail {
    border-top: var(--border-width) solid var(--color-border);
    padding: var(--space-2) var(--space-3);
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
    border-left: 3px solid oklch(0.55 0.16 25);
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

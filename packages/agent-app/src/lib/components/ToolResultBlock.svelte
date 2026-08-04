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

  // engine 格式: "[Tool execute_command result]: {...}"
  // assistant 格式: 纯 JSON
  let parsed = $derived.by(() => {
    const match = message.content.match(/^\[Tool (.+?) result\]:\s*/);
    const jsonStr = match
      ? message.content.slice(match[0].length)
      : message.content;
    const name = match ? match[1] : null;
    try {
      const v = JSON.parse(jsonStr);
      if (v && typeof v === "object") {
        return { name, result: v as CmdResult };
      }
    } catch {
      // fall through to plain text
    }
    return { name, result: null };
  });

  let toolName = $derived(parsed.name ?? t("toolResult.executed"));
  let result = $derived(parsed.result);
  let isCmdShape = $derived(
    result !== null &&
      ("exit_code" in result || "stdout" in result || "stderr" in result)
  );
  let isSuccess = $derived(result?.exit_code === 0);
</script>

{#if isCmdShape}
  <div class="toolresult-block" class:expanded>
    <button class="summary" onclick={toggle}>
      <span class="toggle-icon">{expanded ? "▾" : "▸"}</span>
      <span class="label">🖥 {toolName}</span>
      {#if result!.timed_out}
        <span class="badge timeout">{t("toolResult.timedOut")}</span>
      {/if}
      <span class="exit" class:error={!isSuccess}>{result!.exit_code}</span>
    </button>

    {#if expanded}
      <div class="detail">
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
      </div>
    {/if}
  </div>
{:else}
  <pre class="output fallback">{message.content}</pre>
{/if}

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
    background: oklch(0.15 0.005 75);
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

<script lang="ts">
  import type { HookJudgementRecord } from "$lib/types";
  import { t } from "$lib/i18n";
  import CopyButton from "./CopyButton.svelte";

  let {
    record,
    hookLabel = "",
  }: { record: HookJudgementRecord; hookLabel?: string } = $props();

  let expanded = $state(false);

  const isPending = $derived(record.status === "pending");

  const statusLabel = $derived(
    record.status === "pending"
      ? t("judgement.status.pending")
      : record.status === "ok"
        ? t("judgement.status.ok")
        : record.status === "retried_ok"
          ? t("judgement.status.retriedOk")
          : t("judgement.status.downgraded"),
  );

  /** 悬停语义提示（桌面增强，触屏可点击展开详情）。 */
  const tooltipLabel = $derived(
    record.status === "pending"
      ? t("judgement.tooltip.pending")
      : record.status === "ok"
        ? t("judgement.tooltip.ok")
        : record.status === "retried_ok"
          ? t("judgement.tooltip.retriedOk")
          : t("judgement.tooltip.downgraded"),
  );

  /** pending 态动态耗时：从 created_at 起算，每 500ms 刷新。 */
  let now = $state(Date.now());
  $effect(() => {
    if (!isPending) return;
    const id = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(id);
  });

  function formatElapsed(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
    return `${(ms / 60000).toFixed(1)}m`;
  }

  function parseAttempts(raw: string): { attempt: number; raw: string; error?: string | null }[] {
    try {
      const parsed = JSON.parse(raw);
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      return [];
    }
  }

  function prettyJson(raw: string | null | undefined): string {
    if (!raw) return "";
    try {
      return JSON.stringify(JSON.parse(raw), null, 2);
    } catch {
      return raw;
    }
  }

  // 复制内容 = 完整裁决记录（含决策/错误/明细），与工具卡复制完整负载一致。
  let copyText = $derived(JSON.stringify(record, null, 2));
</script>

<div class="judgement-card" class:expanded>
  <div class="block-header">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="summary"
      role="button"
      tabindex="0"
      onclick={() => (expanded = !expanded)}
      onkeydown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          expanded = !expanded;
        }
      }}
    >
      {#if isPending}
        <span class="badge-ico pending" aria-hidden="true">◌</span>
      {/if}
      <span class="hook-name" title={record.hook_type}>{hookLabel}</span>
      {#if isPending}
        <span class="elapsed">{formatElapsed(now - record.created_at)}</span>
      {/if}
      <span class="block-header-actions" onclick={(e) => e.stopPropagation()}>
        <!-- 裁决的复制：完整记录（含决策/错误/明细） -->
        <CopyButton text={copyText} />
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
      <!-- 展开第一眼：裁决结果 -->
      <div class="verdict" title={tooltipLabel}>
        {#if isPending}
          <span class="badge-ico pending" aria-hidden="true">◌</span>
        {:else if record.status === "ok"}
          <span class="badge-ico ok" aria-hidden="true">✓</span>
        {:else if record.status === "retried_ok"}
          <span class="badge-ico retried_ok" aria-hidden="true">↻</span>
        {:else}
          <span class="badge-ico downgraded" aria-hidden="true">⚠</span>
        {/if}
        <span class="verdict-label {record.status}">{statusLabel}</span>
        {#if isPending}
          <span class="elapsed">{formatElapsed(now - record.created_at)}</span>
        {/if}
      </div>

      <!-- 决策依据 -->
      {#if record.error}
        <p class="reason error">{record.error}</p>
      {:else if record.decision}
        <p class="reason" title={prettyJson(record.decision)}>
          {prettyJson(record.decision).replace(/\s+/g, " ")}
        </p>
      {/if}

      <div class="meta">
        <span>
          {t("judgement.model")}:
          {record.model_provider ?? "-"}/{record.model_id ?? "-"}
        </span>
        <span>{t("judgement.attempts")}: {record.attempts}</span>
        <span>{t("judgement.durationMs")}: {record.duration_ms}</span>
      </div>

      <details class="field">
        <summary>
          <span class="field-chevron" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="m9 18 6-6-6-6" />
            </svg>
          </span>
          {t("judgement.payload")}
        </summary>
        <pre>{prettyJson(record.payload)}</pre>
      </details>

      {#if parseAttempts(record.attempts_detail).length > 0}
        <details class="field">
          <summary>
            <span class="field-chevron" aria-hidden="true">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="m9 18 6-6-6-6" />
              </svg>
            </span>
            {t("judgement.attemptsDetail")}
            <span class="attempts-count">({parseAttempts(record.attempts_detail).length})</span>
          </summary>
          {#each parseAttempts(record.attempts_detail) as attempt}
            <div class="attempt">
              <div class="attempt-head">
                <span class="attempt-no">#{attempt.attempt}</span>
                {#if attempt.error}<span class="attempt-error">{attempt.error}</span>{/if}
              </div>
              <pre>{attempt.raw}</pre>
            </div>
          {/each}
        </details>
      {/if}

      <details class="field">
        <summary>
          <span class="field-chevron" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="m9 18 6-6-6-6" />
            </svg>
          </span>
          {t("judgement.rawResponse")}
        </summary>
        <pre>{record.raw_response}</pre>
      </details>

      {#if record.decision}
        <details class="field">
          <summary>
            <span class="field-chevron" aria-hidden="true">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="m9 18 6-6-6-6" />
              </svg>
            </span>
            {t("judgement.decision")}
          </summary>
          <pre>{prettyJson(record.decision)}</pre>
        </details>
      {/if}

      {#if record.error}
        <details class="field">
          <summary>
            <span class="field-chevron" aria-hidden="true">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="m9 18 6-6-6-6" />
              </svg>
            </span>
            {t("judgement.error")}
          </summary>
          <pre>{record.error}</pre>
        </details>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* 工具类应用：克制。中性表面 + 淡边框，无装饰性色块/竖条/动画。
     左右 margin 对齐消息正文（--space-4 padding），字体颜色与正文一致。 */
  .judgement-card {
    margin: var(--space-2) var(--space-4) 0;
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    border: var(--border-width) solid var(--color-border);
    overflow: hidden;
  }

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
  .summary:hover { background: var(--color-hover); }
  .block-header-actions { flex-shrink: 0; display: inline-flex; align-items: center; }

  /* 折叠行：类型锚点，正文色（与其他卡片折叠行主体一致）。 */
  .hook-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text);
  }

  /* 展开第一眼：结果行——小号字符 + 语义色文字，非加粗。 */
  .verdict {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-bottom: var(--space-1);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .badge-ico {
    flex-shrink: 0;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .badge-ico.ok { color: var(--color-success); }
  .badge-ico.retried_ok { color: var(--color-primary); }
  .badge-ico.downgraded { color: var(--color-warning); }
  .verdict-label {
    flex-shrink: 0;
    color: var(--color-text-muted);
  }
  .verdict-label.ok { color: var(--color-success); }
  .verdict-label.retried_ok { color: var(--color-primary); }
  .verdict-label.downgraded { color: var(--color-warning); }
  .elapsed {
    flex: 0 0 auto;
    font-family: var(--font-mono, monospace);
    color: var(--color-text-muted);
  }

  /* 决策依据：正文色，错误用错误色。 */
  .reason {
    margin: 0;
    color: var(--color-text);
    overflow-wrap: break-word;
    word-break: break-word;
  }
  .reason.error { color: var(--color-error); }

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

  .detail {
    border-top: var(--border-width) solid var(--color-border);
    padding: var(--space-2);
    max-height: 400px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    font-size: var(--fs-xs);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-3);
    color: var(--color-text-muted);
  }
  .field {
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-elevated);
  }
  .field summary {
    list-style: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: var(--space-1) var(--space-2);
    cursor: pointer;
    color: var(--color-text-muted);
    user-select: none;
  }
  .field summary::-webkit-details-marker { display: none; }
  .field summary:hover { color: var(--color-text); }
  .attempts-count { color: var(--color-text-muted); }
  .field-chevron {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    color: var(--color-text-muted);
    transition: transform var(--duration-fast) var(--ease-out);
    transform-origin: center;
  }
  .field-chevron svg { width: 12px; height: 12px; display: block; }
  .field[open] .field-chevron { transform: rotate(90deg); }
  .field pre {
    margin: 0;
    padding: var(--space-2);
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-all;
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    color: var(--color-text);
  }
  .attempt { padding: 0 var(--space-2) var(--space-2); }
  .attempt-head {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    padding: var(--space-1) 0;
  }
  .attempt-no { font-weight: 600; }
  .attempt-error { color: var(--color-error, #c0392b); }
</style>

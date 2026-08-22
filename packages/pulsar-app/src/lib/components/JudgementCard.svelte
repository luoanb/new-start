<script lang="ts">
  import type { HookJudgementRecord } from "$lib/types";
  import { t } from "$lib/i18n";

  let { record }: { record: HookJudgementRecord } = $props();

  let expanded = $state(false);

  const isPending = $derived(record.status === "pending");

  /** 状态徽标：ok=success / retried_ok=primary / downgraded=warning / pending=text-muted。 */
  const tone = $derived(
    record.status === "pending"
      ? "pending"
      : record.status === "ok"
        ? "ok"
        : record.status === "retried_ok"
          ? "retried_ok"
          : "downgraded",
  );

  const statusLabel = $derived(
    record.status === "pending"
      ? t("judgement.status.pending")
      : record.status === "ok"
        ? t("judgement.status.ok")
        : record.status === "retried_ok"
          ? t("judgement.status.retriedOk")
          : t("judgement.status.downgraded"),
  );

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
</script>

<div class="judgement-card {tone}" class:expanded>
  <button type="button" class="card-head" onclick={() => (expanded = !expanded)}>
    {#if isPending}
      <span class="spinner" aria-hidden="true"></span>
    {:else if record.status === "ok"}
      <span class="badge-ico ok">✓</span>
    {:else if record.status === "retried_ok"}
      <span class="badge-ico retried_ok">↻✓</span>
    {:else}
      <span class="badge-ico downgraded">⚠</span>
    {/if}
    <span class="badge-label">{statusLabel}</span>
    {#if !isPending}
      <span class="summary">
        {#if record.error}
          {record.error}
        {:else}
          {prettyJson(record.decision).replace(/\s+/g, " ").slice(0, 120)}
        {/if}
      </span>
    {/if}
    <svg
      class="chevron"
      class:flip={expanded}
      viewBox="0 0 24 24"
      width="12"
      height="12"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <polyline points="6 9 12 15 18 9" />
    </svg>
  </button>

  {#if expanded}
    <div class="card-body">
      <div class="meta">
        <span>
          {t("judgement.model")}:
          {record.model_provider ?? "-"}/{record.model_id ?? "-"}
        </span>
        <span>{t("judgement.attempts")}: {record.attempts}</span>
        <span>{t("judgement.durationMs")}: {record.duration_ms}</span>
      </div>

      <details class="field">
        <summary>{t("judgement.payload")}</summary>
        <pre>{prettyJson(record.payload)}</pre>
      </details>

      {#if parseAttempts(record.attempts_detail).length > 0}
        <details class="field">
          <summary>
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
        <summary>{t("judgement.rawResponse")}</summary>
        <pre>{record.raw_response}</pre>
      </details>

      {#if record.decision}
        <details class="field">
          <summary>{t("judgement.decision")}</summary>
          <pre>{prettyJson(record.decision)}</pre>
        </details>
      {/if}

      {#if record.error}
        <details class="field">
          <summary>{t("judgement.error")}</summary>
          <pre>{record.error}</pre>
        </details>
      {/if}
    </div>
  {/if}
</div>

<style>
  .judgement-card {
    margin: var(--space-2) var(--space-5) 0;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    overflow: hidden;
    font-size: var(--fs-xs);
  }
  .judgement-card.expanded {
    border-color: var(--color-primary);
  }
  .card-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-1) var(--space-2);
    border: none;
    background: transparent;
    color: var(--color-text);
    cursor: pointer;
    text-align: left;
  }
  .card-head:hover {
    background: var(--color-hover);
  }
  .badge-ico {
    flex-shrink: 0;
    font-weight: 700;
    font-size: var(--fs-sm);
  }
  .badge-ico.ok { color: #2ea043; }
  .badge-ico.retried_ok { color: #218bfd; }
  .badge-ico.downgraded { color: #d68910; }
  .badge-label {
    flex-shrink: 0;
    font-weight: 600;
    color: var(--color-text);
  }
  .summary {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-muted);
  }
  .chevron {
    flex-shrink: 0;
    color: var(--color-text-muted);
    transition: transform var(--duration-fast) var(--ease-out);
  }
  .chevron.flip { transform: rotate(180deg); }

  .spinner {
    width: 12px;
    height: 12px;
    flex-shrink: 0;
    border-radius: 50%;
    border: 2px solid var(--color-border);
    border-top-color: var(--color-primary);
    animation: jc-spin 0.9s linear infinite;
  }
  @keyframes jc-spin { to { transform: rotate(360deg); } }

  .card-body {
    border-top: 1px solid var(--color-border);
    padding: var(--space-2);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
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
    background: var(--color-bg);
  }
  .field summary {
    padding: var(--space-1) var(--space-2);
    cursor: pointer;
    color: var(--color-text-muted);
    user-select: none;
  }
  .field summary:hover { color: var(--color-text); }
  .attempts-count { color: var(--color-text-muted); }
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

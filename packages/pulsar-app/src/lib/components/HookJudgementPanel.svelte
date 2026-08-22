<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api, c } from "$lib/api";
  import type { HookDefMeta, HookJudgementRecord } from "$lib/types";
  import { t } from "$lib/i18n";
  import { errorMessage } from "$lib/errorMessage";
  import { useViewContext } from "$lib/layout/viewContext";
  import Select from "./Select.svelte";

  const ctx = useViewContext();

  // ── 数据 ──
  let records = $state<HookJudgementRecord[]>([]);
  let hookDefs = $state<HookDefMeta[]>([]);
  let loading = $state(false);
  let errorMsg = $state("");
  let expandedId = $state<string | null>(null);
  let unsubscribe: (() => void) | null = null;

  // ── 过滤 ──
  let filterHookType = $state(""); // "" = 全部
  let filterStatus = $state(""); // "" = 全部

  /** 状态过滤选项（全部 + 四态）。 */
  const statusOptions = $derived([
    { value: "", label: t("judgement.all") },
    { value: "pending", label: t("judgement.status.pending") },
    { value: "ok", label: t("judgement.status.ok") },
    { value: "retried_ok", label: t("judgement.status.retriedOk") },
    { value: "downgraded", label: t("judgement.status.downgraded") },
  ]);

  /** Hook 类型过滤选项（数据源 = hook_defs_list，label 为 i18n key 由前端解析）。 */
  const hookTypeOptions = $derived([
    { value: "", label: t("judgement.all") },
    ...hookDefs.map((def) => ({ value: def.system_type, label: t(def.label) })),
  ]);

  const filtered = $derived(
    records.filter((r) => {
      if (filterHookType && r.hook_type !== filterHookType) return false;
      if (filterStatus && r.status !== filterStatus) return false;
      return true;
    }),
  );

  /** 全量拉取（后端按 created_at 倒序返回）。 */
  async function refresh() {
    loading = true;
    errorMsg = "";
    try {
      const [list, defs] = await Promise.all([
        api.call(c.hookJudgementsList, {}),
        api.call(c.hookDefsList, undefined),
      ]);
      records = list;
      hookDefs = defs;
    } catch (e) {
      errorMsg = t("judgement.loadFailed", { error: errorMessage(e) });
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void refresh();
    // 两阶段事件驱动：pending（裁决开始）→ 终态（ok/retried_ok/downgraded）。
    // 收到事件后全量重拉，保证列表与详情实时一致（裁决记录量级小，重拉开销可忽略）。
    unsubscribe = api.subscribe((payload) => {
      if (payload.kind === "hook_judgements") void refresh();
    });
  });

  onDestroy(() => {
    unsubscribe?.();
    unsubscribe = null;
  });

  // ── 工具 ──

  function formatTime(tsMs: number): string {
    try {
      return new Intl.DateTimeFormat("zh-CN", {
        timeZone: "Asia/Shanghai",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      }).format(new Date(tsMs));
    } catch {
      return String(tsMs);
    }
  }

  /** hook 展示名（label 是 i18n key；未知类型回退 system_type 原文）。 */
  function hookLabel(record: HookJudgementRecord): string {
    const def = hookDefs.find((d) => d.system_type === record.hook_type);
    return def ? t(def.label) : record.hook_type;
  }

  /** 解析 attempts_detail JSON（全量原文保留，解析失败显示原文）。 */
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

  /** 「在会话中定位」：切换到会话视图 + 滚动高亮锚点消息。 */
  function locate(record: HookJudgementRecord) {
    ctx.commands.selectConversation(record.conversation_id);
    if (record.anchor_message_index != null) {
      ctx.stores.layout.requestLocate(record.conversation_id, record.anchor_message_index);
    }
  }
</script>

<div class="judgement-panel">
  {#if errorMsg}
    <button class="error-banner" type="button" onclick={() => (errorMsg = "")}>{errorMsg}</button>
  {/if}

  <div class="toolbar">
    <label>
      {t("judgement.hookType")}
      <Select
        bind:value={filterHookType}
        options={hookTypeOptions}
        onchange={(v) => (filterHookType = String(v))}
      />
    </label>
    <label>
      {t("judgement.statusLabel")}
      <Select
        bind:value={filterStatus}
        options={statusOptions}
        onchange={(v) => (filterStatus = String(v))}
      />
    </label>
    <button type="button" class="refresh-btn" onclick={() => refresh()} disabled={loading}>
      {loading ? "…" : "↻"}
    </button>
  </div>

  <div class="list">
    {#if filtered.length === 0}
      <p class="empty">{t("judgement.empty")}</p>
    {:else}
      {#each filtered as record (record.id)}
        <div
          class="record {record.status}"
          class:expanded={expandedId === record.id}
        >
          <button
            type="button"
            class="row"
            onclick={() => (expandedId = expandedId === record.id ? null : record.id)}
          >
            <span class="status-badge {record.status}">
              {record.status === "pending"
                ? t("judgement.status.pending")
                : record.status === "ok"
                  ? t("judgement.status.ok")
                  : record.status === "retried_ok"
                    ? t("judgement.status.retriedOk")
                    : t("judgement.status.downgraded")}
            </span>
            <span class="hook-type" title={record.hook_type}>{hookLabel(record)}</span>
            <span class="time">{formatTime(record.created_at)}</span>
            <span class="summary">
              {#if record.status === "pending"}
                <span class="pending-dot" aria-hidden="true"></span>
              {:else if record.error}
                {record.error}
              {:else}
                {record.decision ? prettyJson(record.decision).replace(/\s+/g, " ").slice(0, 80) : ""}
              {/if}
            </span>
            <svg
              class="chevron"
              class:flip={expandedId === record.id}
              viewBox="0 0 24 24"
              width="14"
              height="14"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <polyline points="6 9 12 15 18 9" />
            </svg>
          </button>

          {#if expandedId === record.id}
            <div class="detail">
              <div class="detail-meta">
                <span>{t("judgement.conversation")}: <code>{record.conversation_id}</code></span>
                <span>{t("judgement.attempts")}: {record.attempts}</span>
                <span>{t("judgement.durationMs")}: {record.duration_ms}</span>
                <span>
                  {t("judgement.model")}:
                  {record.model_provider ?? "-"}/{record.model_id ?? "-"}
                </span>
                {#if record.anchor_message_index != null}
                  <button type="button" class="locate-btn" onclick={() => locate(record)}>
                    {t("judgement.locate")}
                  </button>
                {/if}
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
                  {#each parseAttempts(record.attempts_detail) as attempt, i}
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
      {/each}
    {/if}
  </div>
</div>

<style>
  .judgement-panel {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: var(--space-2);
    padding: var(--space-2);
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: end;
  }
  .toolbar label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .refresh-btn {
    height: 28px;
    padding: 0 8px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--fs-xs);
  }
  .empty {
    padding: var(--space-4);
    color: var(--color-text-muted);
  }
  .record {
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    overflow: hidden;
  }
  .record.expanded {
    border-color: var(--color-primary);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2);
    border: none;
    background: transparent;
    color: var(--color-text);
    cursor: pointer;
    text-align: left;
  }
  .row:hover {
    background: var(--color-hover);
  }
  .status-badge {
    flex-shrink: 0;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    white-space: nowrap;
  }
  .status-badge.pending { background: var(--color-hover); color: var(--color-text-muted); }
  .status-badge.ok { background: rgba(46, 160, 67, 0.16); color: #2ea043; }
  .status-badge.retried_ok { background: rgba(33, 139, 253, 0.16); color: #218bfd; }
  .status-badge.downgraded { background: rgba(214, 137, 16, 0.18); color: #d68910; }
  .hook-type {
    flex-shrink: 0;
    color: var(--color-text);
    font-weight: 500;
  }
  .time {
    flex-shrink: 0;
    color: var(--color-text-muted);
  }
  .summary {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-muted);
  }
  .pending-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-primary);
    animation: blink 1s ease-in-out infinite;
    vertical-align: middle;
  }
  @keyframes blink { 0%, 100% { opacity: 0.3; } 50% { opacity: 1; } }
  .chevron {
    flex-shrink: 0;
    color: var(--color-text-muted);
    transition: transform var(--duration-fast) var(--ease-out);
  }
  .chevron.flip { transform: rotate(180deg); }

  .detail {
    border-top: 1px solid var(--color-border);
    padding: var(--space-2);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    font-size: var(--fs-xs);
  }
  .detail-meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-3);
    color: var(--color-text-muted);
  }
  .detail-meta code {
    font-family: var(--font-mono, monospace);
    color: var(--color-text);
  }
  .locate-btn {
    padding: 2px 8px;
    border: var(--border-width) solid var(--color-primary);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-primary);
    cursor: pointer;
  }
  .locate-btn:hover {
    background: var(--color-primary);
    color: var(--color-bg);
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
  .attempt {
    padding: 0 var(--space-2) var(--space-2);
  }
  .attempt-head {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    padding: var(--space-1) 0;
  }
  .attempt-no { font-weight: 600; }
  .attempt-error { color: var(--color-error, #c0392b); }
  .error-banner {
    font-size: var(--fs-xs);
    color: var(--color-error, #c0392b);
    cursor: pointer;
  }
</style>

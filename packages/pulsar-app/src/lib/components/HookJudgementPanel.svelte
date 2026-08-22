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

  const hasFilter = $derived(filterHookType !== "" || filterStatus !== "");

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

  /** 列表条目短时间戳（HH:mm:ss）；完整时间用于 title 悬停。 */
  function formatTimeShort(tsMs: number): string {
    try {
      return new Intl.DateTimeFormat("zh-CN", {
        timeZone: "Asia/Shanghai",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      }).format(new Date(tsMs));
    } catch {
      return String(tsMs);
    }
  }

  function formatTimeFull(tsMs: number): string {
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

  /** 状态文本（i18n 映射）。 */
  function statusLabelOf(record: HookJudgementRecord): string {
    switch (record.status) {
      case "pending":
        return t("judgement.status.pending");
      case "ok":
        return t("judgement.status.ok");
      case "retried_ok":
        return t("judgement.status.retriedOk");
      default:
        return t("judgement.status.downgraded");
    }
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

  <!-- 面板标题栏：对齐 ToolPanel / TopicPanel 的 panel-toolbar 词汇 -->
  <div class="panel-toolbar">
    <span class="panel-title">{t("views.hookJudgements")}</span>
    <div class="toolbar-actions">
      <button
        class="icon-btn"
        onclick={() => refresh()}
        disabled={loading}
        title={t("judgement.refresh")}
        aria-label={t("judgement.refresh")}
      >
        <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>
      </button>
    </div>
  </div>

  <!-- 过滤条：类型 / 状态下拉 + 结果计数 -->
  <div class="filter-bar">
    <Select
      bind:value={filterHookType}
      options={hookTypeOptions}
      onchange={(v) => (filterHookType = String(v))}
    />
    <Select
      bind:value={filterStatus}
      options={statusOptions}
      onchange={(v) => (filterStatus = String(v))}
    />
    <span class="count">{filtered.length}</span>
  </div>

  <div class="list">
    {#if filtered.length === 0}
      <p class="empty">{hasFilter ? t("judgement.noMatch") : t("judgement.empty")}</p>
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
            <span class="time" title={formatTimeFull(record.created_at)}>
              {formatTimeShort(record.created_at)}
            </span>
            <span class="hook-badge" title={record.hook_type}>{hookLabel(record)}</span>
            <span class="status-badge {record.status}">{statusLabelOf(record)}</span>
            <span class="summary-txt">
              {#if record.error}
                {record.error}
              {:else if record.decision}
                {prettyJson(record.decision).replace(/\s+/g, " ").slice(0, 80)}
              {/if}
            </span>
            <span class="toggle-icon" aria-hidden="true">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="m9 18 6-6-6-6" />
              </svg>
            </span>
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
                  <button type="button" class="btn btn-sm locate-btn" onclick={() => locate(record)}>
                    {t("judgement.locate")}
                  </button>
                {/if}
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
      {/each}
    {/if}
  </div>
</div>

<style>
  /* 面板容器：对齐 ToolPanel / TopicPanel 间距（padding / gap / flex 约束） */
  .judgement-panel {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: var(--space-2);
    padding: var(--space-2);
    overflow: auto;
  }
  .error-banner {
    background: var(--color-error);
    color: #fff;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-md);
    font-size: var(--fs-xs);
    cursor: pointer;
  }
  /* 面板标题栏：对齐 panel-toolbar / panel-title / toolbar-actions / icon-btn 词汇 */
  .panel-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .panel-title {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--color-text);
  }
  .toolbar-actions {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .icon-btn {
    flex-shrink: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    width: 26px;
    height: 26px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out);
  }
  .icon-btn:hover:not(:disabled) { background: var(--color-hover); color: var(--color-text); }
  .icon-btn:disabled { opacity: 0.4; cursor: default; }
  .icon-btn .icon { display: block; }
  /* 过滤条：surface 底 + 圆角容器（对齐 TopicPanel filter-bar 词汇） */
  .filter-bar {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 2px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }
  .filter-bar .count {
    margin-left: auto;
    padding: 0 var(--space-1);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--fs-sm);
  }
  .empty {
    padding: var(--space-4);
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
  }
  /* 时间线条目卡片：对齐 topic-card（bg 底 + radius-md） */
  .record {
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    overflow: hidden;
  }
  .record.expanded { border-color: var(--color-primary); }
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
  .row:hover { background: var(--color-hover); }
  .time {
    flex-shrink: 0;
    font-size: var(--fs-xs);
    font-family: var(--font-mono, monospace);
    color: var(--color-text-muted);
  }
  .hook-badge {
    flex-shrink: 0;
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    white-space: nowrap;
  }
  /* 状态徽标：克制——小号文字 + 语义色文字色，无底色/圆点/动画。 */
  .status-badge {
    flex-shrink: 0;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    white-space: nowrap;
  }
  .status-badge.ok { color: var(--color-success); }
  .status-badge.retried_ok { color: var(--color-primary); }
  .status-badge.downgraded { color: var(--color-warning); }
  .summary-txt {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
  }
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
    color: var(--color-primary);
    border-color: color-mix(in oklch, var(--color-primary) 35%, transparent);
  }
  .locate-btn:hover:not(:disabled) {
    background: color-mix(in oklch, var(--color-primary) 10%, transparent);
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
</style>

<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api, c } from "$lib/api";
  import type { HookDefMeta, HookJudgementRecord } from "$lib/types";
  import { t } from "$lib/i18n";
  import { errorMessage } from "$lib/errorMessage";
  import { useViewContext } from "$lib/layout/viewContext";
  import Select from "./Select.svelte";

  const ctx = useViewContext();

  // ── 数据（分页：后端过滤 + 滚动自动加载）──
  const PAGE_SIZE = 50;
  let records = $state<HookJudgementRecord[]>([]);
  /** 过滤后总数（后端 COUNT，支撑计数与 hasMore）。 */
  let total = $state(0);
  let hasMore = $state(false);
  let hookDefs = $state<HookDefMeta[]>([]);
  let loading = $state(false);
  let loadingMore = $state(false);
  let errorMsg = $state("");
  let expandedId = $state<string | null>(null);
  let listEl = $state<HTMLDivElement | null>(null);
  let unsubscribe: (() => void) | null = null;

  // ── 过滤（下沉后端，改动即重置重拉）──
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

  const hasFilter = $derived(filterHookType !== "" || filterStatus !== "");

  /** 构造分页过滤入参（过滤条件下沉后端，limit/offset 走滚动分页）。 */
  function buildFilter(offset: number) {
    return {
      filters: {
        limit: PAGE_SIZE,
        offset,
        ...(filterHookType ? { hookType: filterHookType } : {}),
        ...(filterStatus ? { status: filterStatus } : {}),
      },
    };
  }

  /**
   * 分页拉取：reset=true 清空并拉第一页（过滤变更 / 刷新 / 事件重拉）；
   * reset=false 追加下一页（滚动到底触发）。hasMore 由 records.length < total 判定。
   */
  async function loadPage(reset = false) {
    if (!reset && !hasMore) return;
    const offset = reset ? 0 : records.length;
    loading = reset;
    loadingMore = !reset;
    errorMsg = "";
    try {
      const [res, defs] = await Promise.all([
        api.call(c.hookJudgementsList, buildFilter(offset)),
        reset ? api.call(c.hookDefsList, undefined) : Promise.resolve(hookDefs),
      ]);
      records = reset ? res.records : [...records, ...res.records];
      total = res.total;
      hasMore = records.length < res.total;
      hookDefs = defs;
    } catch (e) {
      errorMsg = t("judgement.loadFailed", { error: errorMessage(e) });
    } finally {
      loading = false;
      loadingMore = false;
    }
  }

  /** 滚动距底 < 80px 自动加载下一页（未在加载中且有更多时）。 */
  function onScroll() {
    if (!listEl) return;
    const el = listEl;
    if (el.scrollHeight - el.scrollTop - el.clientHeight < 80) {
      if (!loading && !loadingMore && hasMore) void loadPage(false);
    }
  }

  /** 过滤变化：重置第一页 + 列表滚动回顶（分页上下文重开）。 */
  function applyFilter(key: "hookType" | "status", value: string) {
    if (key === "hookType") {
      filterHookType = value;
    } else {
      filterStatus = value;
    }
    if (listEl) listEl.scrollTop = 0;
    void loadPage(true);
  }

  onMount(() => {
    void loadPage(true);
    // 两阶段事件驱动：pending（裁决开始）→ 终态（ok/retried_ok/downgraded）。
    // 收到事件后重置重拉首页，保证列表与计数实时一致。
    unsubscribe = api.subscribe((payload) => {
      if (payload.kind === "hook_judgements") void loadPage(true);
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
    <span class="panel-title">{t("views.flowDecisions")}</span>
    <div class="toolbar-actions">
      <button
        class="icon-btn"
        onclick={() => loadPage(true)}
        disabled={loading}
        title={t("judgement.refresh")}
        aria-label={t("judgement.refresh")}
      >
        <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>
      </button>
    </div>
  </div>

  <!-- 过滤条：类型 / 状态下拉 + 结果计数（total = 过滤后总数） -->
  <div class="filter-bar">
    <Select
      bind:value={filterHookType}
      options={hookTypeOptions}
      onchange={(v) => applyFilter("hookType", String(v))}
    />
    <Select
      bind:value={filterStatus}
      options={statusOptions}
      onchange={(v) => applyFilter("status", String(v))}
    />
    <span class="count">{total}</span>
  </div>

  <div class="list" bind:this={listEl} onscroll={onScroll}>
    {#if records.length === 0}
      <p class="empty">{hasFilter ? t("judgement.noMatch") : t("judgement.empty")}</p>
    {:else}
      {#each records as record (record.id)}
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
      {#if loadingMore}
        <p class="list-footer">{t("judgement.loadingMore")}</p>
      {:else if hasMore}
        <p class="list-footer">{t("judgement.loadedOf", { loaded: records.length, total })}</p>
      {:else}
        <p class="list-footer">{t("judgement.allLoaded", { total })}</p>
      {/if}
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
    /* hidden：滚动交由 .list 单层容器，避免双层 overflow 嵌套导致滚动条错位、行被挤没。 */
    overflow: hidden;
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
  /* 分页底部提示：已载入 / 总数；居中、弱化，不占滚动空间。 */
  .list-footer {
    margin: 0;
    padding: var(--space-2);
    text-align: center;
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
    /* 紧凑行高：列表页密度优先，让更多行可见。 */
    padding: 3px var(--space-2);
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
    font-size: var(--fs-sm);
    color: var(--color-text);
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

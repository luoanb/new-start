<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api, c } from "$lib/api";
  import type { HookDefMeta, HookEntry, HookJudgementRecord, HookParamView } from "$lib/types";
  import { t } from "$lib/i18n";
  import { errorMessage } from "$lib/errorMessage";
  import { useViewContext } from "$lib/layout/viewContext";
  import Select from "./Select.svelte";
  import Toggle from "./Toggle.svelte";

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
    void loadHooks();
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

  // ── 周期（动作清单）：完全由后端声明驱动渲染，面板零领域硬编码 ──
  let activeTab = $state<"hooks" | "records">("hooks");
  let hooks = $state<HookEntry[]>([]);
  let hooksLoading = $state(false);
  let hooksError = $state("");
  let savingKey = $state<string | null>(null);
  let expandedHookId = $state<string | null>(null);

  /** 枚举取值展示名（后端给的是稳定值；label 由面板 i18n 映射）。 */
  const VALUE_KEYS: Record<string, string> = {
    chat: "cycle.valueChat",
    agent: "cycle.valueAgent",
    assistant: "cycle.valueAssistant",
    system: "cycle.valueSystem",
    user_round: "cycle.valueUserRound",
    scheduled_round: "cycle.valueScheduledRound",
    tool_round: "cycle.valueToolRound",
    settling_round: "cycle.valueSettlingRound",
  };
  function valueLabel(value: string): string {
    const key = VALUE_KEYS[value];
    return key ? t(key) : value;
  }
  function asBool(value: unknown): boolean {
    return value === true;
  }
  function asNum(value: unknown): number {
    return typeof value === "number" ? value : 0;
  }
  function asArr(value: unknown): string[] {
    return Array.isArray(value) ? value : [];
  }

  async function loadHooks() {
    hooksLoading = true;
    hooksError = "";
    try {
      hooks = await api.call(c.hooksList, undefined);
    } catch (e) {
      hooksError = t("cycle.loadFailed", { error: errorMessage(e) });
    } finally {
      hooksLoading = false;
    }
  }

  /** 启停：统一可切（无硬保护）；失败回滚到服务端状态。 */
  async function toggleHook(entry: HookEntry, on: boolean) {
    savingKey = `${entry.id}:enabled`;
    hooksError = "";
    try {
      await api.call(c.hookSetEnabled, { id: entry.id, on });
      await loadHooks();
    } catch (e) {
      hooksError = t("cycle.saveFailed", { error: errorMessage(e) });
      await loadHooks();
    } finally {
      savingKey = null;
    }
  }

  /** 取值：按后端声明的形态提交（enum 多选 → 数组；单选 → 字符串；bool → 布尔；number → 数字）。 */
  async function setHookValue(entry: HookEntry, param: HookParamView, value: unknown) {
    const key = `${entry.id}:${param.key}`;
    savingKey = key;
    hooksError = "";
    try {
      await api.call(c.hookSetValue, { id: entry.id, key: param.key, value: value as never });
      await loadHooks();
    } catch (e) {
      hooksError = t("cycle.saveFailed", { error: errorMessage(e) });
      await loadHooks();
    } finally {
      savingKey = null;
    }
  }

  function toggleEnumMulti(entry: HookEntry, param: HookParamView, value: string) {
    const current = asArr(param.value);
    const next = current.includes(value)
      ? current.filter((v) => v !== value)
      : [...current, value];
    void setHookValue(entry, param, next);
  }
</script>

{#snippet paramRow(entry: HookEntry, param: HookParamView)}
  <!-- 宽控件（枚举）转上下布局避免挤压；窄控件（开关 / 数值）保持左右 -->
  <div class="param-row" class:stacked={param.kind.kind === "enum"}>
    <span class="param-label">{t(param.label)}</span>
    {#if param.kind.kind === "bool"}
      <span
        class="toggle-wrap"
        onchange={(e) => setHookValue(entry, param, (e.target as HTMLInputElement).checked)}
      >
        <Toggle
          checked={asBool(param.value)}
          disabled={savingKey === `${entry.id}:${param.key}`}
        />
      </span>
    {:else if param.kind.kind === "number"}
      <input
        class="param-number"
        type="number"
        min={param.kind.min}
        max={param.kind.max}
        value={asNum(param.value)}
        disabled={savingKey === `${entry.id}:${param.key}`}
        onchange={(e) => {
          const raw = Number((e.currentTarget as HTMLInputElement).value);
          if (Number.isFinite(raw)) void setHookValue(entry, param, raw);
        }}
      />
    {:else if param.kind.multi}
      <div class="chips">
        {#each param.kind.values as value (value)}
          <button
            type="button"
            class="chip"
            class:on={asArr(param.value).includes(value)}
            disabled={savingKey === `${entry.id}:${param.key}`}
            onclick={() => toggleEnumMulti(entry, param, value)}
          >{valueLabel(value)}</button>
        {/each}
      </div>
    {:else}
      <Select
        value={typeof param.value === "string" ? param.value : ""}
        options={param.kind.values.map((v) => ({ value: v, label: valueLabel(v) }))}
        onchange={(v) => setHookValue(entry, param, String(v))}
      />
    {/if}
  </div>
{/snippet}

{#snippet paramGroup(entry: HookEntry, usage: "call_gate" | "internal", title: string)}
  {@const list = entry.params.filter((p) => p.usage === usage)}
  {#if list.length > 0}
    <p class="group-title">{title}</p>
    {#each list as param (param.key)}
      {@render paramRow(entry, param)}
    {/each}
  {/if}
{/snippet}

<div class="judgement-panel">
  {#if errorMsg}
    <button class="error-banner" type="button" onclick={() => (errorMsg = "")}>{errorMsg}</button>
  {/if}

  <!-- 面板标题栏：对齐 ToolPanel / TopicPanel 的 panel-toolbar 词汇 -->
  <div class="panel-toolbar">
    <span class="panel-title">{t("views.cycleManagement")}</span>
    <div class="toolbar-actions">
      <button
        class="icon-btn"
        onclick={() => (activeTab === "hooks" ? loadHooks() : loadPage(true))}
        disabled={activeTab === "hooks" ? hooksLoading : loading}
        title={t("judgement.refresh")}
        aria-label={t("judgement.refresh")}
      >
        <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>
      </button>
    </div>
  </div>

  <!-- 分区 tab：动作（周期）/ 执行记录 -->
  <div class="tabs">
    <button
      type="button"
      class="tab"
      class:active={activeTab === "hooks"}
      onclick={() => (activeTab = "hooks")}
    >{t("cycle.tabHooks")}</button>
    <button
      type="button"
      class="tab"
      class:active={activeTab === "records"}
      onclick={() => (activeTab = "records")}
    >{t("cycle.tabRecords")}</button>
  </div>

  {#if activeTab === "hooks"}
    <!-- 动作清单：完全由 hooks_list 声明驱动（启停 + 参数），无领域硬编码 -->
    <div class="list hook-list">
      {#if hooksError}
        <button class="error-banner" type="button" onclick={() => (hooksError = "")}>
          {hooksError}
        </button>
      {/if}
      {#if hooks.length === 0}
        <p class="empty">{t("cycle.empty")}</p>
      {:else}
        {#each hooks as entry (entry.id)}
          <div class="hook-row" class:expanded={expandedHookId === entry.id}>
            <button
              type="button"
              class="hook-head"
              onclick={() => (expandedHookId = expandedHookId === entry.id ? null : entry.id)}
            >
              <span class="hook-text">
                <span class="hook-label">{t(entry.label)}</span>
                <span class="hook-meta">
                  <span>{t(entry.group)}</span>
                  <span class="dot">·</span>
                  <span class="hook-ip" title={entry.injectPoint}>{entry.injectPoint}</span>
                </span>
              </span>
              {#if !entry.enabled}
                <span class="hook-state">{t("cycle.stateDisabled")}</span>
              {/if}
              <span class="chevron" class:open={expandedHookId === entry.id} aria-hidden="true">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>
              </span>
            </button>

            {#if expandedHookId === entry.id}
              <div class="hook-detail">
                <!-- 启停开关：Toggle 内部为原生 checkbox，change 冒泡到包裹层 -->
                <div class="param-row">
                  <span class="param-label">{t("cycle.enabled")}</span>
                  <span
                    class="toggle-wrap"
                    onchange={(e) => toggleHook(entry, (e.target as HTMLInputElement).checked)}
                  >
                    <Toggle
                      checked={entry.enabled}
                      disabled={savingKey === `${entry.id}:enabled`}
                    />
                  </span>
                </div>

                {#if entry.disableHint}
                  <p class="hook-warn">{t(entry.disableHint)}</p>
                {/if}

                {#if entry.params.length === 0}
                  <p class="detail-note">{t("cycle.noParams")}</p>
                {:else}
                  {@render paramGroup(entry, "call_gate", t("cycle.schedule"))}
                  {@render paramGroup(entry, "internal", t("cycle.params"))}
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  {:else}
    <!-- 执行记录：过滤条 + 单层滚动列表（原「流程决策」账本时间线） -->
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
            <span class="hook-badge" title={record.hook_type}>{hookLabel(record)}</span>
            <span class="time" title={formatTimeFull(record.created_at)}>
              {formatTimeShort(record.created_at)}
            </span>
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
  {/if}
</div>

<style>
  /* 面板容器：对齐 ToolPanel / TopicPanel 间距（padding / gap / flex 约束） */
  .judgement-panel {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: var(--space-2);
    /* 右侧无 padding：滚动条贴面板右边缘（右边距已在 .list 内部提供）。 */
    padding: var(--space-2) 0 var(--space-2) var(--space-2);
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
    /* 头部右侧留白，与 .list 内部右边距对齐（外层右侧无 padding）。 */
    padding-right: var(--space-2);
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
    border-radius: var(--radius-md);
    background: var(--color-surface);
    /* 与 .list 内部右边距对齐（外层右侧无 padding）。 */
    margin-right: var(--space-2);
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
    gap: var(--space-2);
    font-size: var(--fs-sm);
    /* 内容与滚动条之间留出右边距（滚动条在 padding 外侧，参考 SessionList）。 */
    padding-right: var(--space-2);
  }
  .empty {
    padding: var(--space-4);
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
  }
  /* 分页底部提示：已载入 / 总数；居中、弱化，不占滚动空间。 */
  .list-footer {
    flex-shrink: 0;
    margin: 0;
    padding: var(--space-2);
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
  }
  /* 列表条目（无卡片）：flex-shrink:0 关键——.list 是 flex 列 + overflow 滚动容器，
     不加此行时记录多会按 flex-shrink:1 压缩行高，内容叠在一起而非滚动。 */
  .record {
    flex-shrink: 0;
  }
  /* 展开态：无卡片边框后以行背景高亮（hover 已有）。 */
  .record.expanded .row { background: color-mix(in oklch, var(--color-primary) 10%, transparent); }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    /* 行高/圆角对齐其他 panel（TopicPanel 卡片标准）。 */
    padding: var(--space-2) var(--space-2);
    border: none;
    border-radius: var(--radius-md);
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

  /* ── 周期（动作）区样式：通栏扁平 + token ── */
  .tabs {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-2) var(--space-1) var(--space-2);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .tab {
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    padding: var(--space-1) var(--space-2);
    font-size: var(--fs-sm);
    color: var(--color-text-muted);
    cursor: pointer;
  }
  .tab:hover { background: var(--color-hover); color: var(--color-text); }
  .tab.active {
    background: color-mix(in oklch, var(--color-primary) 14%, transparent);
    color: var(--color-text);
  }
  .hook-row {
    flex-shrink: 0;
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .hook-row:last-child { border-bottom: none; }
  /* 行间用 hairline 分隔，不用 .list 的卡片间距；
     左溢面板根容器的内边距 + 右侧不留白 → hover 背景直达面板左右边缘 */
  .hook-list {
    gap: 0;
    padding-right: 0;
    margin-left: calc(-1 * var(--space-2));
  }
  .hook-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-1) var(--space-2);
    background: none;
    border: none;
    color: var(--color-text);
    cursor: pointer;
    text-align: left;
  }
  .hook-head:hover { background: var(--color-hover); }
  .hook-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .hook-label {
    font-size: var(--fs-sm);
    line-height: 1.35;
  }
  .hook-label,
  .hook-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hook-meta {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .hook-meta .dot { opacity: 0.5; }
  .hook-ip {
    font-family: var(--font-mono, monospace);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hook-state {
    flex-shrink: 0;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .chevron {
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
  .chevron svg { width: 12px; height: 12px; display: block; }
  .chevron.open { transform: rotate(90deg); }
  .hook-warn {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--color-warning);
  }
  .hook-detail {
    padding: var(--space-1) var(--space-2) var(--space-2);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .group-title {
    margin: var(--space-1) 0 0 0;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--color-text-muted);
  }
  .detail-note {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .param-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .param-row.stacked {
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-1);
  }
  .param-label {
    font-size: var(--fs-xs);
    color: var(--color-text);
  }
  .param-number {
    width: 72px;
    padding: 2px 6px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--fs-xs);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .chip {
    padding: 2px 8px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    cursor: pointer;
  }
  .chip:hover:not(:disabled) { color: var(--color-text); }
  .chip.on {
    border-color: transparent;
    background: color-mix(in oklch, var(--color-primary) 14%, transparent);
    color: var(--color-primary);
  }
  .chip:disabled { opacity: 0.5; cursor: default; }
  .toggle-wrap {
    display: inline-flex;
    align-items: center;
  }
</style>

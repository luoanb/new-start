<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api, c, isTauriEnv } from "$lib/api";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import type { LogEntry, LogLevel, PhaseInfo } from "$lib/types";
  import { t } from "$lib/i18n";
  import Select from "./Select.svelte";
  import SuggestInput from "./SuggestInput.svelte";
  import { CopyToClipboard } from "$lib/utils";

  const LEVELS: LogLevel[] = ["error", "warn", "info", "debug", "trace"];
  const LEVEL_RANK: Record<LogLevel, number> = {
    error: 5,
    warn: 4,
    info: 3,
    debug: 2,
    trace: 1,
  };

  let entries = $state<LogEntry[]>([]);
  let verbosity = $state<LogLevel>("info");
  let filterLevel = $state<LogLevel>("info");
  let filterTarget = $state("");
  let filterKeyword = $state("");
  let filterPhase = $state<string>("");
  let phaseOptions = $state<{ value: string; label: string }[]>([]);
  // 详情展开：记录被展开的日志条目（唯一，行内内联展开）。
  let detailEntry = $state<LogEntry | null>(null);
  let logDir = $state<string | null>(null);
  let errorMsg = $state("");
  let unlisten: UnlistenFn | null = null;

  function phaseOf(entry: LogEntry): string {
    return entry.fields?.phase ?? "";
  }

  // 分组前缀，让无分组的 Select 也能靠 label 呈现分组层级。
  function phaseLabel(p: { group: string; label: string }): string {
    return `${p.group} · ${p.label}`;
  }

  const filtered = $derived(
    entries.filter((entry) => {
      if (LEVEL_RANK[entry.level] < LEVEL_RANK[filterLevel]) return false;
      if (
        filterTarget.trim() &&
        !entry.target.toLowerCase().includes(filterTarget.trim().toLowerCase())
      ) {
        return false;
      }
      const ph = filterPhase.trim().toLowerCase();
      if (ph && !phaseOf(entry).toLowerCase().includes(ph)) return false;
      const kw = filterKeyword.trim().toLowerCase();
      if (kw) {
        const fieldText = entry.fields
          ? Object.entries(entry.fields)
              .map(([k, v]) => `${k}=${v}`)
              .join(" ")
          : "";
        const hay = `${entry.message} ${entry.target} ${fieldText}`.toLowerCase();
        if (!hay.includes(kw)) return false;
      }
      return true;
    }).slice(-500),
  );

  onMount(async () => {
    try {
      const [snapshot, level, dir] = await Promise.all([
        api.call(c.logsSnapshot, undefined),
        api.call(c.logsGetLevel, undefined),
        api.call(c.logsDir, undefined),
      ]);
      entries = snapshot;
      verbosity = (LEVELS.includes(level as LogLevel) ? level : "info") as LogLevel;
      filterLevel = verbosity;
      logDir = dir;
      // 后端统一管理的 phase 注册表（唯一事实来源）。
      try {
        const phases = (await api.call(c.logsPhases, undefined)) as PhaseInfo[];
        phaseOptions = phases.map((p) => ({ value: p.value, label: phaseLabel(p) }));
      } catch {
        // 后端未支持时退化为仅"全部"，不影响面板其余功能。
      }
      // app://logs 为 Tauri 专属实时日志流；非 Tauri 环境（远程模式）无该事件源，跳过订阅。
      if (isTauriEnv) {
        unlisten = await listen<LogEntry>("app://logs", (event) => {
          entries = [...entries, event.payload].slice(-2000);
        });
      }
    } catch (e) {
      errorMsg = t("logPanel.initFailed", { error: `${e}` });
    }
  });

  onDestroy(() => {
    if (phaseDebounceTimer) clearTimeout(phaseDebounceTimer);
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
  });

  // phase 搜索框：过滤条件 = 输入框当前值（包含匹配），做防抖避免敲字时频繁过滤。
  // 下拉候选仅作快捷输入：点选/回车只是把候选 value 回填进输入框。
  let phaseInput = $state("");
  let phaseDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  const phaseSuggestions = $derived(
    phaseOptions.map((o) => ({ value: o.value, label: o.label })),
  );
  $effect(() => {
    // 注意：必须在 effect 主体内同步读取 phaseInput 以建立依赖追踪，
    // 否则 Svelte 5 这个 effect 只在挂载时运行一次，filterPhase 永不更新。
    const input = phaseInput;
    if (phaseDebounceTimer) clearTimeout(phaseDebounceTimer);
    phaseDebounceTimer = setTimeout(() => {
      filterPhase = input;
    }, 250);
  });

  function toggleDetail(entry: LogEntry) {
    detailEntry = detailEntry === entry ? null : entry;
  }

  function formatDetail(entry: LogEntry): string {
    const parts: string[] = [];
    if (entry.fields) {
      for (const [k, v] of Object.entries(entry.fields)) {
        if (k === "message") continue;
        parts.push(`${k}=${v}`);
      }
    }
    return parts.join("\n");
  }

  let copyFailed = $state(false);
  let copyOk = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;
  async function copyDetail() {
    if (!detailEntry) return;
    const ok = await CopyToClipboard.copyText(formatDetail(detailEntry));
    if (copyTimer) clearTimeout(copyTimer);
    copyOk = ok;
    copyFailed = !ok;
    copyTimer = setTimeout(() => {
      copyOk = false;
      copyFailed = false;
    }, 1500);
  }

  async function setVerbosity(level: LogLevel) {
    errorMsg = "";
    try {
      const next = await api.call(c.logsSetLevel, { level });
      verbosity = (LEVELS.includes(next as LogLevel) ? next : level) as LogLevel;
    } catch (e) {
      errorMsg = t("logPanel.setLevelFailed", { error: `${e}` });
    }
  }

  async function clearBuffer() {
    errorMsg = "";
    try {
      await api.call(c.logsClearBuffer, undefined);
      entries = [];
    } catch (e) {
      errorMsg = t("logPanel.clearFailed", { error: `${e}` });
    }
  }

  function formatTime(tsMs: number): string {
    try {
      // 显式指定东八区：Tauri WebView（WebKitGTK）时区探测可能回退 UTC，
      // toLocaleTimeString() 会按错误时区展示，这里强制国内时间并带日期。
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

  function fieldSummary(entry: LogEntry): string {
    if (!entry.fields || Object.keys(entry.fields).length === 0) return "";
    return Object.entries(entry.fields)
      .map(([k, v]) => `${k}=${v}`)
      .join(" ");
  }
</script>

<div class="log-panel">
  {#if errorMsg}
    <button class="error-banner" type="button" onclick={() => (errorMsg = "")}>{errorMsg}</button>
  {/if}

  <div class="toolbar">
    <label>
      {t("logPanel.verbosity")}
      <Select
        class="toolbarSelect"
        value={verbosity}
        options={LEVELS.map((level) => ({ value: level, label: level }))}
        onchange={(v) => setVerbosity(v as LogLevel)}
      />
    </label>
    <label>
      {t("logPanel.minLevel")}
      <Select
        class="toolbarSelect"
        bind:value={filterLevel}
        options={LEVELS.map((level) => ({ value: level, label: level }))}
      />
    </label>
    <!-- 用 toolSurface 覆盖 SuggestInput 输入框背景，使与 target/keyword 输入框一致（--color-surface） -->
    <label class="grow">
      {t("logPanel.phase")}
      <SuggestInput
        class="phase-suggest toolSurface"
        clearable
        bind:value={phaseInput}
        suggestions={phaseSuggestions}
        placeholder={t("logPanel.phaseSearch")}
      />
    </label>
    <label class="grow">
      {t("logPanel.target")}
      <input type="text" placeholder={t("logPanel.targetPlaceholder")} bind:value={filterTarget} />
    </label>
    <label class="grow">
      {t("logPanel.keyword")}
      <input type="text" placeholder={t("logPanel.keywordPlaceholder")} bind:value={filterKeyword} />
    </label>
    <button type="button" onclick={clearBuffer}>{t("logPanel.clear")}</button>
  </div>

  {#if logDir}
    <div class="meta">{t("logPanel.file")}: {logDir}</div>
  {/if}

  <div class="list">
    {#if filtered.length === 0}
      <p class="empty">{t("logPanel.empty")}</p>
    {:else}
      {#each filtered as entry}
        <div class="row level-{entry.level}">
          <span class="ts">{formatTime(entry.ts_ms)}</span>
          <span class="phase" title={phaseOf(entry)}>{phaseOf(entry)}</span>
          <span class="level">{entry.level}</span>
          <span class="target" title={entry.target}>{entry.target.split("::").slice(-1)[0]}</span>
          <span class="msg">{entry.message}</span>
          {#if entry.fields && Object.keys(entry.fields).length > 0}
            <span class="fields">
              <button type="button" class="detail-toggle" onclick={() => toggleDetail(entry)}>
                {t("logPanel.detail")}
              </button>
              <span class="summary">{fieldSummary(entry)}</span>
            </span>
          {/if}
          {#if detailEntry === entry}
            <div class="detail">
              <div class="detail-actions">
                {#if copyOk}
                  {t("logPanel.copied")}
                {:else if copyFailed}
                  {t("logPanel.copyFailed")}
                {:else}
                  <button type="button" onclick={copyDetail}>{t("logPanel.copy")}</button>
                {/if}
                <button type="button" onclick={() => (detailEntry = null)}>{t("logPanel.close")}</button>
              </div>
              <pre class="detail-body">{formatDetail(entry)}</pre>
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .log-panel {
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
    min-width: 120px;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .toolbar label.grow { flex: 1; min-width: 120px; }
  .toolbar input,
  .toolbar button {
    font-size: var(--fs-xs);
    padding: 4px 6px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
  }
  .toolbar button { cursor: pointer; height: 28px; }
  .toolbar input:focus {
    border-color: var(--color-primary);
    outline: none;
  }
  /* 让 Select（verbosity / minLevel 触发按钮）与输入框高度一致 */
  :global(.toolbar .toolbarSelect .trigger) {
    font-size: var(--fs-xs);
    padding: 4px 6px;
    line-height: 1.4;
    background: var(--color-surface);
    min-height: 0;
  }
  /* 让 phase SuggestInput 输入框与 target/keyword 输入框完全一致。
     toolSurface 与 .suggest-input 位于同一元素（并列），故选择器须用 .toolSurface .input */
  :global(.toolSurface .input) {
    font-size: var(--fs-xs);
    padding: 4px 6px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
  }
  :global(.toolSurface .input:focus) {
    border-color: var(--color-primary);
    outline: none;
  }
  .meta {
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    word-break: break-all;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: var(--fs-xs);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
  }
  .row {
    display: grid;
    grid-template-columns: 72px 100px 44px 1fr;
    gap: 6px;
    padding: 3px 6px;
    border-bottom: 1px solid var(--color-border);
    align-items: start;
  }
  .row .fields,
  .row .detail {
    grid-column: 2 / -1;
  }
  .row .fields {
    display: flex;
    align-items: baseline;
    gap: 6px;
    color: var(--color-text-muted);
    opacity: 0.9;
  }
  .row .fields .summary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .detail-toggle {
    flex-shrink: 0;
    font-size: var(--fs-xs);
    padding: 1px 6px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-primary);
    cursor: pointer;
  }
  .detail {
    padding-top: var(--space-1);
  }
  .detail-actions {
    display: flex;
    gap: var(--space-2);
    margin-bottom: 4px;
  }
  .detail-actions button {
    font-size: var(--fs-xs);
    padding: 2px 8px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
  }
  .detail-body {
    margin: 0;
    max-height: 240px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-all;
    font-size: var(--fs-xs);
    background: var(--color-bg);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: var(--space-2);
  }
  .phase {
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-xs);
  }
  .ts { color: var(--color-text-muted); }
  .level { font-weight: 600; text-transform: uppercase; }
  .level-error .level { color: var(--color-error); }
  .level-warn .level { color: var(--color-warning); }
  .level-info .level { color: var(--color-primary); }
  .level-debug .level,
  .level-trace .level { color: var(--color-text-muted); }
  .target {
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .msg { word-break: break-word; }
  .empty {
    padding: var(--space-4);
    color: var(--color-text-muted);
  }
  .error-banner {
    font-size: var(--fs-xs);
    color: var(--color-error);
    cursor: pointer;
  }
</style>

<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api, isTauriEnv } from "$lib/api";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import type { LogEntry, LogLevel } from "$lib/types";
  import Select from "./Select.svelte";

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
  let logDir = $state<string | null>(null);
  let errorMsg = $state("");
  let unlisten: UnlistenFn | null = null;

  const filtered = $derived(
    entries.filter((entry) => {
      if (LEVEL_RANK[entry.level] < LEVEL_RANK[filterLevel]) return false;
      if (
        filterTarget.trim() &&
        !entry.target.toLowerCase().includes(filterTarget.trim().toLowerCase())
      ) {
        return false;
      }
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
        api.invoke<LogEntry[]>("logs_snapshot"),
        api.invoke<string>("logs_get_level"),
        api.invoke<string | null>("logs_dir"),
      ]);
      entries = snapshot;
      verbosity = (LEVELS.includes(level as LogLevel) ? level : "info") as LogLevel;
      filterLevel = verbosity;
      logDir = dir;
      // app://logs 为 Tauri 专属实时日志流；非 Tauri 环境（远程模式）无该事件源，跳过订阅。
      if (isTauriEnv) {
        unlisten = await listen<LogEntry>("app://logs", (event) => {
          entries = [...entries, event.payload].slice(-2000);
        });
      }
    } catch (e) {
      errorMsg = `Logs init failed: ${e}`;
    }
  });

  onDestroy(() => {
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
  });

  async function setVerbosity(level: LogLevel) {
    errorMsg = "";
    try {
      const next = await api.invoke<string>("logs_set_level", { level });
      verbosity = (LEVELS.includes(next as LogLevel) ? next : level) as LogLevel;
    } catch (e) {
      errorMsg = `Set level failed: ${e}`;
    }
  }

  async function clearBuffer() {
    errorMsg = "";
    try {
      await api.invoke("logs_clear_buffer");
      entries = [];
    } catch (e) {
      errorMsg = `Clear failed: ${e}`;
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
      Verbosity
      <Select
        value={verbosity}
        options={LEVELS.map((level) => ({ value: level, label: level }))}
        onchange={(v) => setVerbosity(v as LogLevel)}
      />
    </label>
    <label>
      Min level
      <Select
        bind:value={filterLevel}
        options={LEVELS.map((level) => ({ value: level, label: level }))}
      />
    </label>
    <label class="grow">
      Target
      <input type="text" placeholder="neuron / gateway…" bind:value={filterTarget} />
    </label>
    <label class="grow">
      Keyword
      <input type="text" placeholder="phase / error_code…" bind:value={filterKeyword} />
    </label>
    <button type="button" onclick={clearBuffer}>Clear</button>
  </div>

  {#if logDir}
    <div class="meta">file: {logDir}</div>
  {/if}

  <div class="list">
    {#if filtered.length === 0}
      <p class="empty">No log entries match the current filters.</p>
    {:else}
      {#each filtered as entry}
        <div class="row level-{entry.level}">
          <span class="ts">{formatTime(entry.ts_ms)}</span>
          <span class="level">{entry.level}</span>
          <span class="target" title={entry.target}>{entry.target.split("::").slice(-1)[0]}</span>
          <span class="msg">{entry.message}</span>
          {#if fieldSummary(entry)}
            <span class="fields">{fieldSummary(entry)}</span>
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
    grid-template-columns: 72px 44px 110px 1fr;
    gap: 6px;
    padding: 3px 6px;
    border-bottom: 1px solid var(--color-border);
    align-items: start;
  }
  .row .fields {
    grid-column: 2 / -1;
    color: var(--color-text-muted);
    opacity: 0.9;
  }
  .ts { color: var(--color-text-muted); }
  .level { font-weight: 600; text-transform: uppercase; }
  .level-error .level { color: #c0392b; }
  .level-warn .level { color: #d68910; }
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
    color: #c0392b;
    cursor: pointer;
  }
</style>

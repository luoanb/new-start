<script lang="ts">
  // CommitDiff：main 区「commit-diff」面板，查看某提交中单个文件的改动（历史 diff）。
  // - 实例 key = `commit-diff:${repoId}:${hash}:${path}`（openCommitDiff 生成），经 ViewHost 注入的 context 取回。
  // - 数据源：git_show_diff（hash + path → GitFileDiff）。
  // - 渲染与 GitDiff 统一：hunk 头、行号列、增删行色，面板内独立滚动。
  import { onMount, getContext } from "svelte";
  import { api, c } from "$lib/api";
  import { t } from "$lib/i18n";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import type { MainPanel } from "$lib/layout/layoutTypes";
  import type { GitFileDiff } from "$lib/types";

  // 实例解析：key = `commit-diff:${repoId}:${hash}:${path}`（repoId / hash 均不含冒号，path 可含冒号）。
  const panel = getContext<MainPanel>("pulsar:panel");
  const raw = panel.id.startsWith("commit-diff:") ? panel.id.slice("commit-diff:".length) : panel.id;
  const sep = raw.indexOf(":");
  const repoId = sep >= 0 ? raw.slice(0, sep) : raw;
  const rest = sep >= 0 ? raw.slice(sep + 1) : "";
  const sep2 = rest.indexOf(":");
  const hash = sep2 >= 0 ? rest.slice(0, sep2) : rest;
  const path = sep2 >= 0 ? rest.slice(sep2 + 1) : "";

  let diff = $state<GitFileDiff | null>(null);
  let loading = $state(true);
  let error = $state("");

  async function load() {
    loading = true;
    error = "";
    try {
      diff = await api.call(c.gitShowDiff, { hash, path });
    } catch (e) {
      diff = null;
      error = formatInvokeError(e);
    }
    loading = false;
  }

  onMount(() => void load());

  function basename(p: string): string {
    const i = p.lastIndexOf("/");
    return i >= 0 ? p.slice(i + 1) : p;
  }

  const shortHash = $derived(hash.slice(0, 7));

  /** 从 hunks 行统计 +/- 行数（numstat 语义简化版）。 */
  const stats = $derived.by(() => {
    let add = 0;
    let del = 0;
    for (const h of diff?.hunks ?? []) {
      for (const ln of h.lines) {
        if (ln.kind === "add") add++;
        else if (ln.kind === "del") del++;
      }
    }
    return { add, del };
  });
</script>

<div class="commit-diff">
  <div class="head">
    <span class="path" title={path}>{basename(path)}</span>
    <span class="meta">{shortHash}</span>
    {#if diff && !diff.is_binary}
      <span class="stats"><i class="add">+{stats.add}</i> <i class="del">-{stats.del}</i></span>
    {/if}
  </div>

  {#if error}
    <p class="error-bar">{error}</p>
  {:else if loading}
    <p class="hint">{t("git.logLoading")}</p>
  {:else if diff?.is_binary}
    <p class="hint">{t("git.binaryDiff")}</p>
  {:else if !diff || diff.hunks.length === 0}
    <p class="hint">{t("git.diffEmpty")}</p>
  {:else}
    <div class="body">
      {#each diff.hunks as h (h.header)}
        <div class="hunk">
          <div class="hunk-head">{h.header}</div>
          {#each h.lines as ln, i (i)}
            <div class="line {ln.kind}">
              <span class="no old">{ln.old_no ?? ""}</span>
              <span class="no new">{ln.new_no ?? ""}</span>
              <span class="marker">{ln.kind === "add" ? "+" : ln.kind === "del" ? "-" : " "}</span>
              <span class="txt">{ln.text.slice(1)}</span>
            </div>
          {/each}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .commit-diff {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-surface);
    font-size: var(--fs-sm);
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 36px;
    padding: var(--space-1) var(--space-3);
    border-bottom: var(--border-width) solid var(--color-border);
    flex-shrink: 0;
  }
  .path {
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    color: var(--color-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    white-space: nowrap;
  }
  .stats {
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    margin-left: auto;
  }
  .stats i { font-style: normal; }
  .stats .add { color: var(--color-success); }
  .stats .del { color: var(--color-error); }

  .error-bar {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    font-size: var(--fs-xs);
    color: var(--color-error);
    background: var(--color-error-bg);
  }
  .hint {
    padding: var(--space-4);
    text-align: center;
    font-size: var(--fs-sm);
    color: var(--color-text-muted);
  }

  .body {
    flex: 1;
    overflow: auto;
    padding-bottom: var(--space-4);
  }
  .hunk-head {
    padding: 2px var(--space-3);
    background: var(--color-elevated);
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    white-space: nowrap;
    position: sticky;
    top: 0;
  }
  .line {
    display: flex;
    align-items: stretch;
    white-space: pre;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    line-height: 1.5;
  }
  .line.add { background: color-mix(in srgb, var(--color-success) 12%, transparent); }
  .line.del { background: color-mix(in srgb, var(--color-error) 12%, transparent); }
  .no {
    flex: none;
    width: 3.5em;
    padding-right: var(--space-2);
    text-align: right;
    color: var(--color-text-muted);
    user-select: none;
  }
  .marker {
    flex: none;
    width: 1.5em;
    text-align: center;
    color: var(--color-text-muted);
    user-select: none;
  }
  .txt {
    flex: 1;
    min-width: 0;
  }
</style>

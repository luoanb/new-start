<script lang="ts">
  // GitDiff：main 区「git-diff」面板，按文件路径多实例（复用 file-editor 实例机制）。
  // - 实例 key = `git-diff:${repoId}:${relPath}`（openGitDiff 生成），经 ViewHost 注入的 context 取回。
  // - 数据源：git_diff（repoId 定位仓库，cached 切换 staged/unstaged，both = 合并两次拉取）。
  // - unified 行内渲染：hunk 头（--color-elevated 底）、行号列、增删行色（error/success 12% 混合）。
  // - 冲突文件：冲突标记解析为 ours/theirs 两块 + 接受按钮（ours/theirs/both）→ git_resolve_conflict。
  // - blame 模式：同面板内切换，拉取 git_blame 渲染行首 blame 栏。
  // - hunk 导航：上一处/下一处 + n/total 计数 + 当前 hunk 摘要，滚动到对应 hunk。
  import { onMount, getContext } from "svelte";
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import { api } from "$lib/api";
  import { t } from "$lib/i18n";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import type { MainPanel } from "$lib/layout/layoutTypes";
  import type {
    GitBlameLine,
    GitDiff,
    GitDiffLine,
    GitFileDiff,
    GitHunk,
    GitStatusView,
    ConflictTake,
  } from "$lib/types";
  import Select from "./Select.svelte";

  // ── 实例解析：key = `git-diff:${repoId}:${relPath}:${range}`（repoId 无冒号；旧 key 无 range 尾段 → unstaged）──
  const panel = getContext<MainPanel>("pulsar:panel");
  const raw = panel.id.startsWith("git-diff:") ? panel.id.slice("git-diff:".length) : panel.id;
  const rangeIdx = raw.lastIndexOf(":");
  let rangePart = "";
  let rest = raw;
  if (rangeIdx >= 0) {
    const tail = raw.slice(rangeIdx + 1);
    if (tail === "staged" || tail === "unstaged" || tail === "both") {
      rangePart = tail;
      rest = raw.slice(0, rangeIdx);
    }
  }
  const sep = rest.indexOf(":");
  const repoId = sep >= 0 ? rest.slice(0, sep) : rest;
  const relPath = sep >= 0 ? rest.slice(sep + 1) : "";

  const repo = $derived(dataStore.state.git?.repos.find((r) => r.id === repoId) ?? null);
  /** 该文件在仓库状态中的条目（判断冲突/未跟踪/二进制）。 */
  const statusEntry = $derived.by((): { status: string } | null => {
    const st: GitStatusView | null | undefined = dataStore.state.git?.statusByRepo[repoId];
    if (!st) return null;
    for (const group of [st.conflicted, st.staged, st.unstaged, st.untracked]) {
      const hit = group.find((e) => e.path === relPath);
      if (hit) return { status: hit.status };
    }
    return null;
  });
  const isConflicted = $derived(!!statusEntry?.status.includes("U"));
  const isUntracked = $derived(statusEntry?.status === "??");

  // ── 范围（staged/unstaged/both）──
  type Range = "staged" | "unstaged" | "both";
  /** 打开来源分组的默认范围：暂存 → staged，工作区 → unstaged，冲突 → both。 */
  const initialRange: Range = rangePart === "staged" ? "staged" : rangePart === "both" ? "both" : "unstaged";
  let range = $state<Range>(initialRange);
  const rangeOptions = $derived([
    { value: "staged", label: t("git.rangeStaged") },
    { value: "unstaged", label: t("git.rangeUnstaged") },
    { value: "both", label: t("git.rangeBoth") },
  ]);
  const rangeLabel = $derived(
    range === "staged" ? t("git.rangeStaged") : range === "unstaged" ? t("git.rangeUnstaged") : t("git.rangeBoth"),
  );

  // ── 数据 ──
  let diff = $state<GitDiff | null>(null);
  let blame = $state<GitBlameLine[] | null>(null);
  let loading = $state(false);
  let error = $state("");
  let blameMode = $state(false);

  async function loadDiff() {
    loading = true;
    error = "";
    try {
      if (range === "both") {
        const [s, u] = await Promise.all([
          api.invoke<GitDiff>("git_diff", { repoId, path: relPath, cached: true }),
          api.invoke<GitDiff>("git_diff", { repoId, path: relPath, cached: false }),
        ]);
        diff = mergeDiffs(s, u);
      } else {
        diff = await api.invoke<GitDiff>("git_diff", { repoId, path: relPath, cached: range === "staged" });
      }
    } catch (e) {
      diff = null;
      error = formatInvokeError(e);
    }
    loading = false;
  }

  async function loadBlame() {
    if (blame !== null) return;
    loading = true;
    error = "";
    try {
      blame = await api.invoke<GitBlameLine[]>("git_blame", { repoId, path: relPath });
    } catch (e) {
      error = formatInvokeError(e);
    }
    loading = false;
  }

  function setRange(v: string) {
    if (v === range) return;
    range = v as Range;
    void loadDiff();
  }

  function toggleBlame() {
    blameMode = !blameMode;
    if (blameMode) void loadBlame();
  }

  /** both 范围：按文件合并 staged + unstaged 的 hunks（同一文件两个范围各自独立 hunk 列表）。 */
  function mergeDiffs(a: GitDiff, b: GitDiff): GitDiff {
    const files = new Map<string, GitFileDiff>();
    for (const d of [a, b]) {
      for (const f of d.files) {
        const cur = files.get(f.path);
        if (cur) {
          cur.hunks = [...cur.hunks, ...f.hunks];
          cur.is_binary ||= f.is_binary;
        } else {
          files.set(f.path, { ...f, hunks: [...f.hunks] });
        }
      }
    }
    return { files: [...files.values()], truncated: a.truncated || b.truncated };
  }

  // ── 冲突 hunk 解析：marker 行（<<<<<<< / ======= / >>>>>>>）拆分为 ours/theirs 两块 ──
  type ConflictBlock = { kind: "conflict"; ours: GitDiffLine[]; theirs: GitDiffLine[] };
  type Seg = { kind: "line"; line: GitDiffLine } | ConflictBlock;

  function parseHunk(h: GitHunk): Seg[] {
    const out: Seg[] = [];
    let state: "normal" | "ours" | "theirs" = "normal";
    let ours: GitDiffLine[] = [];
    let theirs: GitDiffLine[] = [];
    for (const ln of h.lines) {
      const text = ln.text.replace(/^[+-]/, "");
      if (/^<<<<<<< /.test(text)) {
        state = "ours";
        ours = [];
        continue;
      }
      if (/^=======/.test(text)) {
        state = "theirs";
        theirs = [];
        continue;
      }
      if (/^>>>>>>> /.test(text)) {
        if (state !== "normal") out.push({ kind: "conflict", ours, theirs });
        state = "normal";
        continue;
      }
      if (state === "ours") ours.push(ln);
      else if (state === "theirs") theirs.push(ln);
      else out.push({ kind: "line", line: ln });
    }
    if (state !== "normal") out.push({ kind: "conflict", ours, theirs });
    return out;
  }

  // ── hunk 导航 ──
  const hunks = $derived(diff?.files[0]?.hunks ?? []);
  const isBinary = $derived(diff?.files[0]?.is_binary ?? false);
  let hunkIndex = $state(0);
  let bodyEl = $state<HTMLElement | null>(null);

  function jumpHunk(i: number) {
    hunkIndex = Math.max(0, Math.min(hunks.length - 1, i));
    const el = bodyEl?.querySelector(`[data-hunk="${hunkIndex}"]`);
    el?.scrollIntoView({ block: "start" });
  }

  // ── 冲突接受 ──
  let resolving = $state(false);
  async function resolveConflict(take: ConflictTake) {
    resolving = true;
    error = "";
    try {
      await dataStore.gitResolveConflict(relPath, take, repoId);
      await loadDiff();
    } catch (e) {
      error = formatInvokeError(e);
    }
    resolving = false;
  }

  onMount(() => {
    void loadDiff();
  });

  function basename(p: string): string {
    const i = p.lastIndexOf("/");
    return i >= 0 ? p.slice(i + 1) : p;
  }
</script>

<div class="git-diff">
  <div class="diff-head">
    <span class="diff-path" title={repo ? `${repo.root}/${relPath}` : relPath}>
      {basename(relPath)}
    </span>
    <span class="diff-range">{rangeLabel}</span>
    <div class="head-actions">
      <Select
        class="range-select"
        value={range}
        options={rangeOptions}
        onchange={(v) => setRange(String(v))}
      />
      <button
        class="btn btn-sm blame-btn"
        class:active={blameMode}
        disabled={isUntracked}
        title={t("git.blame")}
        onclick={toggleBlame}
      >{t("git.blame")}</button>
    </div>
  </div>

  {#if error}
    <p class="error-bar">{error}</p>
  {/if}

  {#if blameMode}
    <!-- blame 视图：行首 blame 栏（hash7 作者 日期）+ 内容 -->
    {#if blame && blame.length > 0}
      <div class="blame-list">
        {#each blame as b (b.line_no)}
          <div class="blame-row">
            <span class="blame-meta" title={`${b.short} · ${b.author} · ${b.date}`}>
              <span class="blame-hash">{b.short}</span>
              <span class="blame-author">{b.author}</span>
              <span class="blame-date">{b.date}</span>
            </span>
            <span class="blame-no">{b.line_no}</span>
            <span class="blame-text">{b.text}</span>
          </div>
        {/each}
      </div>
    {:else if loading}
      <p class="hint">{t("git.blameLoading")}</p>
    {:else}
      <p class="hint">{t("git.diffEmpty")}</p>
    {/if}
  {:else if isUntracked}
    <p class="hint">{t("git.untrackedHint")}</p>
  {:else if isBinary}
    <p class="hint">{t("git.binaryDiff")}</p>
  {:else if hunks.length === 0 && !loading}
    <p class="hint">{t("git.diffEmpty")}</p>
  {:else if hunks.length > 0}
    <!-- hunk 导航条 -->
    <div class="nav">
      <button class="btn btn-sm" disabled={hunkIndex <= 0} onclick={() => jumpHunk(hunkIndex - 1)}>◀ {t("git.prevHunk")}</button>
      <button class="btn btn-sm" disabled={hunkIndex >= hunks.length - 1} onclick={() => jumpHunk(hunkIndex + 1)}>{t("git.nextHunk")} ▶</button>
      <span class="hunk-summary">{hunks[hunkIndex].header}</span>
      <span class="hunk-count">{t("git.hunkCount", { current: hunkIndex + 1, total: hunks.length })}</span>
    </div>

    <!-- unified diff 正文 -->
    <div class="diff-body" bind:this={bodyEl}>
      {#each hunks as h, i (i)}
        <div class="hunk" data-hunk={i}>
          <div class="hunk-head">{h.header}</div>
          {#each parseHunk(h) as seg, j (j)}
            {#if seg.kind === "line"}
              {@const ln = seg.line}
              <div class="line {ln.kind}">
                <span class="no old">{ln.old_no ?? ""}</span>
                <span class="no new">{ln.new_no ?? ""}</span>
                <span class="marker">{ln.kind === "add" ? "+" : ln.kind === "del" ? "-" : " "}</span>
                <span class="txt">{ln.text.slice(1)}</span>
              </div>
            {:else if isConflicted}
              <!-- 冲突块：ours / theirs 两块 + 接受按钮（对齐 VS Code 冲突编辑器） -->
              <div class="conflict-block">
                <div class="conflict-pane ours">
                  <div class="conflict-head">
                    <span class="conflict-label">{t("git.acceptOurs")}</span>
                    <button class="btn btn-sm" disabled={resolving} onclick={() => resolveConflict("ours")}>{t("git.acceptOurs")}</button>
                  </div>
                  <div class="conflict-lines">
                    {#each seg.ours as ln (ln.old_no ?? ln.new_no ?? 0)}
                      <div class="line del">
                        <span class="no old">{ln.old_no ?? ""}</span>
                        <span class="no new">{ln.new_no ?? ""}</span>
                        <span class="marker">-</span>
                        <span class="txt">{ln.text.replace(/^[+-]/, "")}</span>
                      </div>
                    {/each}
                  </div>
                </div>
                <div class="conflict-pane theirs">
                  <div class="conflict-head">
                    <span class="conflict-label">{t("git.acceptTheirs")}</span>
                    <button class="btn btn-sm" disabled={resolving} onclick={() => resolveConflict("theirs")}>{t("git.acceptTheirs")}</button>
                  </div>
                  <div class="conflict-lines">
                    {#each seg.theirs as ln (ln.old_no ?? ln.new_no ?? 0)}
                      <div class="line add">
                        <span class="no old">{ln.old_no ?? ""}</span>
                        <span class="no new">{ln.new_no ?? ""}</span>
                        <span class="marker">+</span>
                        <span class="txt">{ln.text.replace(/^[+-]/, "")}</span>
                      </div>
                    {/each}
                  </div>
                </div>
                <button class="btn btn-sm accept-both" disabled={resolving} onclick={() => resolveConflict("both")}>{t("git.acceptBoth")}</button>
              </div>
            {/if}
          {/each}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .git-diff {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
    background: var(--color-surface);
    font-size: var(--fs-sm);
  }

  /* 面板头部：路径 + 范围标签 + 控制 */
  .diff-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 36px;
    padding: var(--space-1) var(--space-3);
    border-bottom: var(--border-width) solid var(--color-border);
    flex-shrink: 0;
  }
  .diff-path {
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    color: var(--color-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .diff-range {
    flex-shrink: 0;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    white-space: nowrap;
  }
  .head-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex-shrink: 0;
  }
  .range-select :global(.select) {
    min-width: 130px;
  }
  .blame-btn.active {
    color: var(--color-primary);
    border-color: var(--color-primary);
  }

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

  /* hunk 导航条 */
  .nav {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3);
    border-bottom: var(--border-width) solid var(--color-border);
    flex-shrink: 0;
  }
  .hunk-summary {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .hunk-count {
    flex-shrink: 0;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }

  /* diff 正文 */
  .diff-body {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    line-height: 20px;
    overflow-y: auto;
  }
  .hunk {
    margin-bottom: var(--space-1);
  }
  .hunk-head {
    padding: 2px var(--space-3);
    background: var(--color-elevated);
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    font-family: var(--font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .line {
    display: flex;
    align-items: stretch;
    min-height: 20px;
  }
  .line .no {
    flex: 0 0 38px;
    padding-right: var(--space-2);
    text-align: right;
    color: var(--color-text-muted);
    font-size: 11px;
    user-select: none;
  }
  .line .marker {
    flex: 0 0 14px;
    text-align: center;
    color: var(--color-text-muted);
    user-select: none;
  }
  .line .txt {
    flex: 1 1 auto;
    white-space: pre;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .line.context { color: var(--color-text); }
  .line.add { background: color-mix(in oklch, var(--color-success) 12%, transparent); }
  .line.add .marker, .line.add .txt { color: var(--color-success); }
  .line.del { background: color-mix(in oklch, var(--color-error) 12%, transparent); }
  .line.del .marker, .line.del .txt { color: var(--color-error); }

  /* 冲突块：ours / theirs 两块 + 接受按钮 */
  .conflict-block {
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    margin: 0 var(--space-2) var(--space-2);
    overflow: hidden;
  }
  .conflict-pane + .conflict-pane {
    border-top: var(--border-width) solid var(--color-border);
  }
  .conflict-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: 2px var(--space-2);
    background: var(--color-elevated);
  }
  .conflict-label {
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .conflict-lines {
    padding: var(--space-1) 0;
  }
  .accept-both {
    display: block;
    width: calc(100% - var(--space-4));
    margin: var(--space-2) auto;
  }

  /* blame 视图 */
  .blame-list {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    line-height: 20px;
    overflow-x: auto;
  }
  .blame-row {
    display: flex;
    align-items: stretch;
    white-space: pre;
  }
  .blame-row:hover { background: var(--color-hover); }
  .blame-meta {
    flex: 0 0 240px;
    display: flex;
    gap: var(--space-2);
    padding-left: var(--space-3);
    color: var(--color-text-muted);
    font-size: 11px;
    overflow: hidden;
  }
  .blame-hash { color: var(--color-primary); flex-shrink: 0; }
  .blame-author { overflow: hidden; text-overflow: ellipsis; }
  .blame-date { flex-shrink: 0; }
  .blame-no {
    flex: 0 0 38px;
    padding-right: var(--space-2);
    text-align: right;
    color: var(--color-text-muted);
    user-select: none;
  }
  .blame-text { flex: 1 1 auto; color: var(--color-text); }
</style>

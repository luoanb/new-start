<script lang="ts">
  // FileEditor：main 区「文件编辑」面板（多实例，按文件路径区分 tab）。
  // - CodeMirror 6（basicSetup + language-data 按扩展名加载语法高亮）
  // - 打开：fs_read（行分段，truncated 时补齐剩余行）；保存：Ctrl+S / 按钮 → fs_write
  // - 冲突检测：保存前 fs_info 对比打开时 mtime 快照，不一致弹确认（确认后以当前磁盘 mtime 为 base 覆盖）
  // - 未保存标记：docChanged → fileEditorStore.markDirty（tab ● 由 EditorTabs 读取）
  // - 实例 key = panel.id = `${workspaceId}:${relPath}`；编辑器内容由本组件持有，元数据入 fileEditorStore
  import { onMount, onDestroy, getContext } from "svelte";
  import { EditorView, keymap } from "@codemirror/view";
  import { EditorState, Compartment } from "@codemirror/state";
  import { basicSetup } from "codemirror";
  import { languages } from "@codemirror/language-data";
  import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
  import { tags as lezerTags } from "@lezer/highlight";
  import type { LanguageSupport } from "@codemirror/language";
  import { api, c } from "$lib/api";
  import { t } from "$lib/i18n";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import { fileEditorStore, relPathOfKey } from "$lib/stores/fileEditorStore.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import type { MainPanel } from "$lib/layout/layoutTypes";
  import type { FsReadResult, FsWriteResult, FsInfo } from "$lib/types";

  // ViewHost 注入当前面板实例（panel.id = 实例 key）。
  const panel = getContext<MainPanel>("pulsar:panel");
  const key = panel?.id ?? "";
  const relPath = relPathOfKey(key);
  // 重启恢复场景：fileEditorStore 元数据为空，从实例 key 前缀还原工作区 id（uuid 不含 ":"）。
  const workspaceId = $derived(fileEditorStore.workspaceOf(key) || key.split(":")[0]);

  let containerEl: HTMLDivElement | undefined = $state();
  let view: EditorView | null = null;
  const langCompartment = new Compartment();

  /** 读取到的文件内容（null = 未就绪；"" = 空文件也创建编辑器）。 */
  let fileContent = $state<string | null>(null);

  let loadState = $state<"loading" | "ready" | "error">("loading");
  let loadError = $state("");
  let mtimeMs = $state<number | null>(null);
  let totalLines = $state(0);
  let loadedLines = $state(0);
  let saveState = $state<"idle" | "saving" | "saved">("idle");
  let saveError = $state("");
  let conflictReq = $state(false);
  let conflictBase = $state<number | null>(null);

  /** 文件所属工作区不再是 active → 阻止读写（防止写入错误工作区同名文件）。 */
  function workspaceSwitched(): boolean {
    const activeId = dataStore.state.workspaces?.active_id ?? null;
    return !!workspaceId && !!activeId && workspaceId !== activeId;
  }

  async function loadFile(): Promise<void> {
    loadState = "loading";
    loadError = "";
    saveError = "";
    console.log("[FileEditor] load begin", {
      key,
      relPath,
      workspaceId,
      activeId: dataStore.state.workspaces?.active_id ?? null,
      switched: workspaceSwitched(),
    });
    if (workspaceSwitched()) {
      loadState = "error";
      loadError = t("fileEditor.workspaceMismatch");
      return;
    }
    try {
      const first = await api.call(c.fsRead, { path: relPath });
      console.log("[FileEditor] fs_read ok", {
        contentLen: first.content.length,
        totalLines: first.total_lines,
        truncated: first.truncated,
        mtimeMs: first.mtime_ms,
      });
      let content = first.content;
      const firstLines =
        first.content === ""
          ? 0
          : first.content.split("\n").length - (first.content.endsWith("\n") ? 1 : 0);
      if (first.truncated) {
        const remaining = first.total_lines - firstLines;
        if (remaining > 0) {
          const rest = await api.call(c.fsRead, {
            path: relPath,
            offset: firstLines,
            limit: remaining,
          });
          content = first.content + rest.content;
        }
      }
      fileEditorStore.open(key, workspaceId, relPath, first.mtime_ms);
      mtimeMs = first.mtime_ms;
      totalLines = first.total_lines;
      loadedLines = first.total_lines;
      fileContent = content;
      loadState = "ready";
    } catch (e) {
      console.error("[FileEditor] fs_read failed", e);
      loadState = "error";
      loadError = formatInvokeError(e);
    }
  }

  // 内容就绪且宿主元素已挂载时才创建编辑器（bind:this 赋值与 state 渲染是异步的，
  // 原先同步调用 createEditor 时 containerEl 可能尚未绑定 → 静默 return → 内容区空白无报错）。
  $effect(() => {
    if (loadState !== "ready" || !containerEl || fileContent === null) return;
    if (view) view.destroy(); // 重试/重载保护：先销毁旧实例
    view = null;
    console.log("[FileEditor] createEditor", { contentLen: fileContent.length });
    view = new EditorView({
      parent: containerEl,
      state: EditorState.create({
        doc: fileContent,
        extensions: [
          basicSetup,
          appTheme,
          syntaxHighlighting(appHighlight),
          keymap.of([
            { key: "Mod-s", preventDefault: true, run: () => { void save(); return true; } },
          ]),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) fileEditorStore.markDirty(key, true);
          }),
          langCompartment.of([]),
        ],
      }),
    });
    void setupLanguage(relPath);
  });

  /** 按扩展名加载语言高亮（language-data 按需异步加载）。 */
  async function setupLanguage(fileName: string): Promise<void> {
    if (!view) return;
    const ext = fileName.split(".").pop()?.toLowerCase() ?? "";
    const match = languages.find((l) => l.extensions.includes(ext));
    if (!match) return;
    try {
      const support: LanguageSupport = await match.load();
      if (view) view.dispatch({ effects: langCompartment.reconfigure(support) });
    } catch {
      // 语言包加载失败忽略（保持默认高亮）
    }
  }

  /**
   * 编辑器外观跟随应用主题（html[data-theme] 的 CSS 变量）：
   * CM6 默认无颜色主题，行号/背景固定浅色，夜间主题下行号会不可见。
   * 全部用 var(--color-*) 引用，主题切换时随变量自动生效，无需 JS 重新配置。
   */
  const appTheme = EditorView.theme({
    "&": {
      backgroundColor: "var(--color-bg)",
      color: "var(--color-text)",
    },
    ".cm-gutters": {
      backgroundColor: "var(--color-surface)",
      color: "var(--color-text-muted)",
      borderRight: "1px solid var(--color-border)",
    },
    ".cm-gutterElement": {
      color: "var(--color-text-muted)",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "var(--color-hover)",
    },
    ".cm-activeLine": {
      backgroundColor: "color-mix(in oklch, var(--color-primary) 6%, transparent)",
    },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
      backgroundColor: "color-mix(in oklch, var(--color-primary) 25%, transparent)",
    },
    ".cm-cursor": {
      borderLeftColor: "var(--color-primary)",
    },
  });

  /**
   * 语法高亮：用官方 HighlightStyle.define 定义 token 配色，颜色值全部走 CSS 变量（--tok-*），
   * 随应用主题（html[data-theme]）自动切换深浅两套，无需 JS 重配置。
   * 不用 CM6 默认 defaultHighlightStyle：它是面向浅色主题的深色字（keyword #708 等），夜间不可读。
   * 注意：defaultHighlightStyle 的 token 颜色是随机生成类名，无法用 CSS 覆盖，必须自定义 HighlightStyle。
   */
  const appHighlight = HighlightStyle.define([
    { tag: lezerTags.meta, color: "var(--tok-meta)" },
    { tag: lezerTags.keyword, color: "var(--tok-keyword)" },
    { tag: [lezerTags.atom, lezerTags.bool, lezerTags.url, lezerTags.contentSeparator, lezerTags.labelName], color: "var(--tok-atom)" },
    { tag: [lezerTags.literal, lezerTags.inserted], color: "var(--tok-literal)" },
    { tag: [lezerTags.string, lezerTags.deleted], color: "var(--tok-string)" },
    { tag: [lezerTags.regexp, lezerTags.escape, lezerTags.special(lezerTags.string)], color: "var(--tok-string2)" },
    { tag: lezerTags.definition(lezerTags.variableName), color: "var(--tok-var-def)" },
    { tag: lezerTags.local(lezerTags.variableName), color: "var(--tok-var-local)" },
    { tag: [lezerTags.typeName, lezerTags.namespace], color: "var(--tok-type)" },
    { tag: lezerTags.className, color: "var(--tok-class)" },
    { tag: [lezerTags.special(lezerTags.variableName), lezerTags.macroName], color: "var(--tok-var-special)" },
    { tag: lezerTags.definition(lezerTags.propertyName), color: "var(--tok-property)" },
    { tag: lezerTags.comment, color: "var(--tok-comment)" },
    { tag: lezerTags.invalid, color: "var(--tok-invalid)" },
  ]);

  /** 保存：先 fs_info 预检冲突，未冲突直接写；冲突弹确认。 */
  async function save(): Promise<void> {
    if (!view || saveState === "saving" || loadState !== "ready") return;
    saveError = "";
    if (workspaceSwitched()) {
      saveError = t("fileEditor.workspaceMismatch");
      return;
    }
    try {
      const info = await api.call(c.fsInfo, { path: relPath });
      if (!info.exists) {
        saveError = t("fileEditor.fileMissing");
        return;
      }
      if (mtimeMs !== null && info.modified_ms !== null && info.modified_ms !== mtimeMs) {
        conflictBase = info.modified_ms;
        conflictReq = true;
        return;
      }
      await doWrite(info.modified_ms);
    } catch (e) {
      saveError = t("fileEditor.saveFailed", { error: formatInvokeError(e) });
    }
  }

  async function doWrite(baseMtime: number | null): Promise<void> {
    if (!view) return;
    saveState = "saving";
    saveError = "";
    try {
      const content = view.state.doc.toString();
      const res = await api.call(c.fsWrite, {
        path: relPath,
        content,
        base_mtime: baseMtime,
      });
      mtimeMs = res.mtime_ms;
      fileEditorStore.markDirty(key, false);
      fileEditorStore.setMtime(key, res.mtime_ms);
      saveState = "saved";
      setTimeout(() => {
        if (saveState === "saved") saveState = "idle";
      }, 1500);
    } catch (e) {
      saveState = "idle";
      saveError = t("fileEditor.saveFailed", { error: formatInvokeError(e) });
    }
  }

  onMount(() => {
    void loadFile();
  });

  onDestroy(() => {
    view?.destroy();
    view = null;
  });
</script>

<div class="file-editor">
  <div class="editor-toolbar">
    <span class="path" title={relPath}>{relPath}</span>
    {#if saveError}<span class="save-error">{saveError}</span>{/if}
    <div class="spacer"></div>
    <button
      class="btn btn-sm btn-primary"
      disabled={saveState === "saving" || loadState !== "ready"}
      onclick={() => void save()}
    >
      {saveState === "saving"
        ? t("fileEditor.saving")
        : saveState === "saved"
          ? t("fileEditor.saved")
          : t("fileEditor.save")}
    </button>
  </div>

  <div class="editor-body">
    {#if loadState === "loading"}
      <p class="status">{t("fileEditor.loading")}</p>
    {:else if loadState === "error"}
      <div class="status error">
        <p>{loadError}</p>
        <button class="btn btn-sm" onclick={() => void loadFile()}>↻</button>
      </div>
    {:else}
      <div class="cm-host" bind:this={containerEl}></div>
      <div class="editor-statusbar">
        <span>{t("fileEditor.loadedLines", { loaded: loadedLines, total: totalLines })}</span>
        {#if saveState === "saved"}<span class="ok">{t("fileEditor.saved")}</span>{/if}
      </div>
    {/if}
  </div>
</div>

{#if conflictReq}
  <ConfirmDialog
    open
    title={t("fileEditor.conflictTitle")}
    message={t("fileEditor.conflictBody")}
    confirmLabel={t("fileEditor.overwrite")}
    cancelLabel={t("fileEditor.cancel")}
    onConfirm={() => {
      conflictReq = false;
      void doWrite(conflictBase);
    }}
    onCancel={() => {
      conflictReq = false;
      conflictBase = null;
    }}
  />
{/if}

<style>
  .file-editor {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--color-surface);
    /* 语法高亮 token 色（浅色主题默认：对齐 CM6 defaultHighlightStyle 的浅底深字配色）；
       夜间主题在下方 @media/[data-theme] 块整体覆盖为浅色字。 */
    --tok-meta: #404740;
    --tok-keyword: #708;
    --tok-atom: #219;
    --tok-literal: #164;
    --tok-string: #a11;
    --tok-string2: #e40;
    --tok-var-def: #00f;
    --tok-var-local: #30a;
    --tok-type: #085;
    --tok-class: #167;
    --tok-var-special: #256;
    --tok-property: #00c;
    --tok-comment: #940;
    --tok-invalid: #f00;
  }

  .editor-toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-bottom: var(--border-width) solid var(--color-border);
    flex-shrink: 0;
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-xs);
    font-family: var(--font-mono, monospace);
    color: var(--color-text-muted);
  }
  .save-error {
    font-size: var(--fs-xs);
    color: var(--color-error);
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }

  .editor-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .status {
    padding: var(--space-4);
    color: var(--color-text-muted);
    font-size: var(--fs-sm);
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
  }
  .status.error p {
    color: var(--color-error);
    word-break: break-all;
  }

  /* CodeMirror 宿主：滚动由编辑器内部接管 */
  .cm-host {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  :global(.cm-host .cm-editor) {
    height: 100%;
  }
  :global(.cm-host .cm-scroller) {
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-sm);
  }

  .editor-statusbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 2px var(--space-2);
    border-top: var(--border-width) solid var(--color-border);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    flex-shrink: 0;
  }
  .ok {
    color: var(--color-success);
  }

  /* ── 夜间模式整体设计 ──
     - muted 提亮：路径 / 状态栏 / CM 行号在深色表面 (oklch 0.20) 上对比度不足，局部覆盖 token；
     - 语法高亮：CM6 默认 defaultHighlightStyle 是浅底深字（keyword #708 等），夜间不可读，
       组件内以 HighlightStyle.define（见 script）引用 --tok-* 变量，此处整体覆盖为浅色系
       （VS Code Dark+ 近似），手动 dark 与跟随系统暗色两种场景都覆盖。 */
  :global([data-theme="dark"]) .file-editor {
    --color-text-muted: oklch(0.74 0.012 265);
    --tok-meta: #8a919e;
    --tok-keyword: #c678dd;
    --tok-atom: #d19a66;
    --tok-literal: #98c379;
    --tok-string: #98c379;
    --tok-string2: #e06c75;
    --tok-var-def: #61afef;
    --tok-var-local: #61afef;
    --tok-type: #e5c07b;
    --tok-class: #e5c07b;
    --tok-var-special: #d19a66;
    --tok-property: #e06c75;
    --tok-comment: #8a919e;
    --tok-invalid: #ff6b6b;
  }
  @media (prefers-color-scheme: dark) {
    :global(:root:not([data-theme])) .file-editor {
      --color-text-muted: oklch(0.74 0.012 265);
      --tok-meta: #8a919e;
      --tok-keyword: #c678dd;
      --tok-atom: #d19a66;
      --tok-literal: #98c379;
      --tok-string: #98c379;
      --tok-string2: #e06c75;
      --tok-var-def: #61afef;
      --tok-var-local: #61afef;
      --tok-type: #e5c07b;
      --tok-class: #e5c07b;
      --tok-var-special: #d19a66;
      --tok-property: #e06c75;
      --tok-comment: #8a919e;
      --tok-invalid: #ff6b6b;
    }
  }
</style>

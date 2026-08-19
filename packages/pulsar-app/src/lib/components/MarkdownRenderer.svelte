<script lang="ts">
  import { marked } from "marked";
  import DOMPurify from "dompurify";
  import { onDestroy } from "svelte";

  let { content }: { content: string } = $props();

  // Configure marked for link target
  marked.use({
    renderer: {
      link({ href, text }) {
        return `<a href="${href}" target="_blank" rel="noopener noreferrer">${text}</a>`;
      },
      code({ text, lang }) {
        const langLabel = lang
          ? `<span class="code-lang">${lang}</span>`
          : "";
        return `<div class="code-block">${langLabel}<pre><code>${text}</code></pre></div>`;
      },
      codespan({ text }) {
        return `<code class="inline-code">${text}</code>`;
      },
    },
  });

  function escapeHtml(s: string): string {
    return s
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  /**
   * 流式兜底：中间态常出现未闭合的代码块 / 表格，marked 会渲染成残缺 HTML。
   * 检测到未闭合结构时，完整部分照常解析，未闭合尾部以 <pre> 纯文本兜底。
   */
  function renderSafe(src: string): string {
    const fences = src.match(/```/g)?.length ?? 0;
    if (fences % 2 === 1) {
      const idx = src.lastIndexOf("```");
      // 剥离语言标注首行，剩余代码块内容作纯文本兜底。
      const tail = src.slice(idx + 3).replace(/^\s*[^\n]*\n?/, "");
      const headHtml = DOMPurify.sanitize(
        marked.parse(src.slice(0, idx), { async: false }) as string
      );
      return `${headHtml}<div class="code-block"><pre><code>${escapeHtml(tail)}</code></pre></div>`;
    }
    const lines = src.split("\n");
    const last = lines[lines.length - 1];
    if (lines.length > 1 && /^\s*\|.*\|\s*$/.test(last)) {
      const headHtml = DOMPurify.sanitize(
        marked.parse(lines.slice(0, -1).join("\n"), { async: false }) as string
      );
      return `${headHtml}<pre>${escapeHtml(last)}</pre>`;
    }
    return DOMPurify.sanitize(marked.parse(src, { async: false }) as string);
  }

  // 初始同步渲染（无首帧延迟）；后续流式高频更新走 ~150ms 批量合并。
  // svelte-ignore state_referenced_locally -- 初始化刻意只捕获 content 初始值，后续更新由 $effect 节流接管。
  let rendered = $state(renderSafe(content));
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending = "";

  $effect(() => {
    if (content === pending) return;
    pending = content;
    if (timer) return;
    timer = setTimeout(() => {
      timer = undefined;
      const next = renderSafe(pending);
      if (next !== rendered) rendered = next;
    }, 150);
  });

  onDestroy(() => {
    if (timer) clearTimeout(timer);
    timer = undefined;
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="markdown-body">
  {@html rendered}
</div>

<style>
  .markdown-body {
    line-height: 1.6;
    word-break: break-word;
  }

  .markdown-body :global(h1),
  .markdown-body :global(h2),
  .markdown-body :global(h3),
  .markdown-body :global(h4),
  .markdown-body :global(h5),
  .markdown-body :global(h6) {
    margin: 0.6em 0 0.3em;
    font-weight: 600;
    line-height: 1.3;
  }

  .markdown-body :global(h1) { font-size: 1.3em; }
  .markdown-body :global(h2) { font-size: 1.15em; }
  .markdown-body :global(h3) { font-size: 1.05em; }
  .markdown-body :global(h4),
  .markdown-body :global(h5),
  .markdown-body :global(h6) { font-size: 1em; }

  .markdown-body :global(p) {
    margin: 0.4em 0;
  }

  .markdown-body :global(ul),
  .markdown-body :global(ol) {
    margin: 0.4em 0;
    padding-left: 1.6em;
  }

  .markdown-body :global(li) {
    margin: 0.2em 0;
  }

  .markdown-body :global(strong) {
    font-weight: 600;
  }

  .markdown-body :global(em) {
    font-style: italic;
  }

  .markdown-body :global(hr) {
    margin: 0.8em 0;
    border: none;
    border-top: 1px solid var(--color-border);
  }

  .markdown-body :global(blockquote) {
    margin: 0.4em 0;
    padding: 4px 12px;
    border-left: 3px solid var(--color-border);
    color: var(--color-text-muted);
  }

  .markdown-body :global(a) {
    color: var(--color-primary);
    text-decoration: none;
  }

  .markdown-body :global(a:hover) {
    text-decoration: underline;
  }

  .markdown-body :global(table) {
    border-collapse: collapse;
    margin: 0.4em 0;
    font-size: var(--fs-sm);
    max-width: 100%;
    overflow-x: auto;
    display: block;
  }

  .markdown-body :global(th),
  .markdown-body :global(td) {
    padding: 6px 10px;
    border: 1px solid var(--color-border);
    text-align: left;
  }

  .markdown-body :global(th) {
    font-weight: 600;
    background: var(--color-surface);
  }

  .markdown-body :global(.code-block) {
    margin: 0.5em 0;
    border-radius: 8px;
    background: oklch(0.20 0.005 75);
    overflow: hidden;
  }

  .markdown-body :global(.code-lang) {
    display: inline-block;
    padding: 2px 10px;
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: oklch(0.6 0.01 75);
    background: oklch(0.24 0.005 75);
  }

  .markdown-body :global(.code-block pre) {
    margin: 0;
    padding: 12px 14px;
    overflow-x: auto;
  }

  .markdown-body :global(.code-block code) {
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    line-height: 1.5;
    color: oklch(0.88 0.004 75);
    background: transparent;
    padding: 0;
  }

  .markdown-body :global(.inline-code) {
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
  }
</style>

<script lang="ts">
  import type { Message } from "$lib/types";
  import MarkdownRenderer from "./MarkdownRenderer.svelte";
  import ThinkingBlock from "./ThinkingBlock.svelte";
  import ToolCallBlock from "./ToolCallBlock.svelte";
  import ToolResultBlock from "./ToolResultBlock.svelte";
  import NudgeBlock from "./NudgeBlock.svelte";
  import { t } from "$lib/i18n";
  import { clickOutside } from "$lib/actions/clickOutside";

  let {
    message,
    onCopy,
    onRate,
    canRate,
  }: {
    message: Message;
    onCopy?: (msg: Message) => void;
    onRate?: (score: number) => void;
    canRate?: boolean;
  } = $props();

  const isUser = $derived(message.role === "user");
  const isAssistant = $derived(message.role === "assistant");
  const isSystem = $derived(message.role === "system");
  const isCompaction = $derived(message.role === "compaction");
  const isTool = $derived(message.role === "tool");
  const isToolResult = $derived(message.body.kind === "tool_result");
  const isNudge = $derived(message.body.kind === "nudge");
  const isContext = $derived(message.body.kind === "role_context");
  // Q1 统一类型后工具调用平级挂载于 text 变体（wire 同源投影）。
  const hasToolCalls = $derived(
    message.body.kind === "text" && (message.body.tool_calls?.length ?? 0) > 0
  );
  const toolCalls = $derived(
    message.body.kind === "text" && message.body.tool_calls ? message.body.tool_calls : []
  );
  /** 推理模型的思考链（wire `reasoning_content` 同源投影；无思考为空串，不渲染折叠块）。 */
  const reasoning = $derived(
    message.body.kind === "text" ? (message.body.reasoning ?? "") : ""
  );

  // 操作栏显隐：仅系统消息/压缩摘要无操作栏。
  // 工具结果与轮询简报的复制已内嵌在各折叠块头部（CopyButton），此处不再显示；
  // 助手消息（含带 tool_call 的）保留底部复制（仅 content）与评分。
  const showActions = $derived(!isSystem && !isCompaction && !isToolResult && !isNudge && !isContext);

  // 打分区间与模型约束一致：-5..5 且非 0（去掉 0），一行 10 个。
  const scoreList = [-5, -4, -3, -2, -1, 1, 2, 3, 4, 5];

  let ratingOpen = $state(false);
  let ratingBtnEl: HTMLButtonElement | undefined = $state();
  let panelUp = $state(false);
  let copied = $state(false);

  function toggleRating() {
    if (ratingOpen) {
      ratingOpen = false;
      return;
    }
    if (ratingBtnEl) {
      const rect = ratingBtnEl.getBoundingClientRect();
      // 以最近的滚动容器（.messages）为基准：它带 overflow 裁剪，面板超出其
      // 可视底部会被裁掉。按钮下方剩余空间不足一个面板高度时向上弹出。
      const container = ratingBtnEl.closest(".messages") as HTMLElement | null;
      const availableBelow = container
        ? container.getBoundingClientRect().bottom - rect.bottom
        : window.innerHeight - rect.bottom;
      panelUp = availableBelow < 48;
    }
    ratingOpen = true;
  }

  async function handleCopy() {
    await onCopy?.(message);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  function handleRate(score: number) {
    ratingOpen = false;
    onRate?.(score);
  }

  function formatTime(ts: number): string {
    const d = new Date(ts);
    const h = d.getHours().toString().padStart(2, "0");
    const m = d.getMinutes().toString().padStart(2, "0");
    return `${h}:${m}`;
  }
</script>

<div
  class="message"
  class:user={isUser}
  class:assistant={isAssistant}
  class:system={isSystem}
  class:nudge={isNudge}
  class:roleContext={isContext}
>
  <div class="msg-col">
    <div class="bubble">
      <div class="role-bar">
        <span class="role-label">
          {#if isSystem}
            {t("chatMessage.system")}
          {:else if isCompaction}
            {t("chatMessage.compaction")}
          {:else if isNudge}
            {t("chatMessage.nudge")}
          {:else if isContext}
            {t("chatMessage.context")}
          {:else if isTool}
            {t("chatMessage.tool")}
          {:else if isAssistant}
            {t("chatMessage.assistant")}
          {:else}
            {t("chatMessage.you")}
          {/if}
        </span>
        {#if !isSystem}
          <span class="timestamp">{formatTime(message.timestamp)}</span>
        {/if}
      </div>

      {#if isToolResult}
        <ToolResultBlock {message} />
      {:else if hasToolCalls}
        {#if reasoning}
          <ThinkingBlock {reasoning} />
        {/if}
        {#if message.body.content}
          <div class="content markdown-content">
            <MarkdownRenderer content={message.body.content} />
          </div>
        {/if}
        <ToolCallBlock {toolCalls} />
      {:else if isSystem}
        <div class="system-prompt">{message.body.content}</div>
      {:else if isCompaction}
        <div class="content compaction-content">
          <MarkdownRenderer content={message.body.content} />
        </div>
      {:else if isNudge || isContext}
        <NudgeBlock content={message.body.content} />
      {:else}
        {#if reasoning}
          <ThinkingBlock {reasoning} />
        {/if}
        <div class="content markdown-content">
          <MarkdownRenderer content={message.body.content} />
        </div>
      {/if}
    </div>

    {#if showActions}
      <div class="actions">
        <button
          class="action-btn"
          class:copied
          onclick={handleCopy}
          title={copied ? t("chatMessage.copied") : t("chatMessage.copy")}
          aria-label={copied ? t("chatMessage.copied") : t("chatMessage.copy")}
        >
          {#if copied}
            <svg
              viewBox="0 0 24 24"
              width="14"
              height="14"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <polyline points="20 6 9 17 4 12" />
            </svg>
          {:else}
            <svg
              viewBox="0 0 24 24"
              width="14"
              height="14"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
              <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
            </svg>
          {/if}
        </button>
        {#if isAssistant && canRate}
          <div
            class="rating"
            role="group"
            aria-label={t("chatMessage.rate")}
            use:clickOutside={ratingOpen ? () => (ratingOpen = false) : null}
          >
            <button
              class="action-btn"
              bind:this={ratingBtnEl}
              title={t("chatMessage.rate")}
              aria-label={t("chatMessage.rate")}
              onclick={toggleRating}
            >
              <svg
                viewBox="0 0 24 24"
                width="14"
                height="14"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <polygon
                  points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"
                />
              </svg>
            </button>
            {#if ratingOpen}
              <div class="rating-panel" class:up={panelUp}>
                {#each scoreList as score}
                  <button
                    class="rating-btn"
                    onclick={(e) => { e.stopPropagation(); handleRate(score); }}
                  >
                    {score}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .message { display: flex; padding: var(--space-1) var(--space-4); animation: msg-fadein var(--duration-normal) var(--ease-out); }
  @keyframes msg-fadein { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: translateY(0); } }
  .message.user { justify-content: flex-end; }
  .message.assistant, .message.system { justify-content: flex-start; }
  .msg-col { display: flex; flex-direction: column; gap: var(--space-1); width: 100%; min-width: 0; }
  .message.user .msg-col { align-items: flex-end; }
  /* 非 user：子项拉伸全宽，保证折叠块（tool_call/tool_result/nudge/role_context）等宽 */
  .message.assistant .msg-col, .message.system .msg-col, .message.nudge .msg-col, .message.roleContext .msg-col { align-items: stretch; }
  /* 默认气泡：仅 user 使用；其余消息无外壳（透明、无边框、无内边距、全宽） */
  .bubble { max-width: 100%; padding: 0; border-radius: var(--radius-md); background: transparent; border: none; }
  .message.user .bubble { max-width: 75%; padding: var(--space-2) var(--space-3); background: var(--color-primary); color: var(--color-on-primary); border: var(--border-width) solid var(--color-primary); border-bottom-right-radius: var(--space-1); }
  .message.system .bubble { padding: 0; text-align: left; }
  /* 轮询简报 / 角色切换（B2 role context）：容器左对齐，气泡透明无壳（NudgeBlock 自带外壳与边框）；
     需显式重置 user 气泡泄漏（nudge/role_context 消息 role=user，同时命中 .message.user .bubble） */
  .message.nudge, .message.roleContext { justify-content: flex-start; }
  .message.nudge .bubble, .message.roleContext .bubble { max-width: 100%; padding: 0; background: transparent; border: none; color: var(--color-text); }
  .role-bar { display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-1); }
  .role-label { font-size: var(--fs-xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; opacity: 0.6; }
  .timestamp { font-size: var(--fs-xs); opacity: 0.4; }
  .content { font-size: var(--fs-base); line-height: 1.5; }
  /* 系统提示词：参考 TopicPanel 的弱化样式——小字号、muted 色、细边框卡片，降低视觉权重 */
  .system-prompt {
    font-size: var(--fs-sm);
    line-height: 1.5;
    color: var(--color-text-muted);
    background: color-mix(in oklch, var(--color-text-muted) 5%, transparent);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: var(--space-2) var(--space-3);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .content.markdown-content :global(p) { margin: 0.3em 0; }
  .content.markdown-content :global(p:first-child) { margin-top: 0; }
  .content.markdown-content :global(p:last-child) { margin-bottom: 0; }

  .actions { display: inline-flex; align-items: center; gap: var(--space-1); position: relative; transition: opacity var(--duration-fast) var(--ease-out); }
  /* 仅支持 hover 的设备隐藏消息操作栏（hover/键盘聚焦时显示）；
     触屏（hover: none）始终可见，保证可发现性。见 .cursor/rules/ui-hover-reveal.mdc */
  @media (hover: hover) {
    .actions {
      opacity: 0;
      visibility: hidden;
    }
    .message:hover .actions,
    .message:focus-within .actions {
      opacity: 1;
      visibility: visible;
    }
  }
  .action-btn { display: inline-flex; align-items: center; justify-content: center; width: 26px; height: 24px; padding: 0; border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-surface); color: var(--color-text-muted); cursor: pointer; }
  .action-btn:hover { color: var(--color-text); border-color: var(--color-primary); }
  .action-btn.copied { color: var(--color-primary); border-color: var(--color-primary); }
  .rating { position: relative; display: inline-flex; }
  .rating-panel { position: absolute; top: 100%; left: 0; margin-top: 4px; display: grid; grid-template-columns: repeat(10, 26px); gap: 2px; padding: var(--space-1); background: var(--color-surface); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); z-index: 100; }
  .rating-panel.up { top: auto; bottom: 100%; margin-top: 0; margin-bottom: 4px; }
  .message.user .rating-panel { left: auto; right: 0; }
  .rating-btn { font-size: var(--fs-xs); height: 22px; border: 1px solid transparent; border-radius: var(--radius-sm); background: transparent; color: var(--color-text-muted); cursor: pointer; }
  .rating-btn:hover { background: var(--color-primary); color: var(--color-on-primary); }
</style>

<script lang="ts">
  import type { Message } from "$lib/types";
  import MarkdownRenderer from "./MarkdownRenderer.svelte";
  import ToolCallBlock from "./ToolCallBlock.svelte";
  import ToolResultBlock from "./ToolResultBlock.svelte";
  import { t } from "$lib/i18n";

  let { message }: { message: Message } = $props();

  const isUser = $derived(message.role === "user");
  const isAssistant = $derived(message.role === "assistant");
  const isSystem = $derived(message.role === "system");
  const isToolResult = $derived(message.msg_type === "tool_result");
  const hasToolCalls = $derived(
    message.tool_calls && message.tool_calls.length > 0
  );

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
>
  <div class="bubble">
    <div class="role-bar">
      <span class="role-label">
        {#if isSystem}
          {t("chatMessage.system")}
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
    {:else if isSystem}
      <p class="content">{message.content}</p>
    {:else}
      <div class="content markdown-content">
        <MarkdownRenderer content={message.content} />
      </div>
    {/if}

    {#if hasToolCalls}
      <ToolCallBlock toolCalls={message.tool_calls!} />
    {/if}
  </div>
</div>

<style>
  .message { display: flex; padding: var(--space-1) var(--space-4); animation: msg-fadein var(--duration-normal) var(--ease-out); }
  @keyframes msg-fadein { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: translateY(0); } }
  .message.user { justify-content: flex-end; }
  .message.assistant, .message.system { justify-content: flex-start; }
  .bubble { max-width: 75%; padding: var(--space-2) var(--space-3); border-radius: var(--radius-md); background: var(--color-surface); border: var(--border-width) solid var(--color-border); }
  .message.user .bubble { background: var(--color-primary); color: var(--color-on-primary); border-color: var(--color-primary); border-bottom-right-radius: var(--space-1); }
  .message.assistant .bubble { border-bottom-left-radius: var(--space-1); border: none; max-width: 100%; }
  .message.system .bubble { background: transparent; border: none; text-align: center; max-width: 100%; padding: var(--space-1) var(--space-3); }
  .role-bar { display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-1); }
  .role-label { font-size: var(--fs-xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; opacity: 0.6; }
  .timestamp { font-size: var(--fs-xs); opacity: 0.4; }
  .content { font-size: var(--fs-base); line-height: 1.5; }
  .content.markdown-content :global(p) { margin: 0.3em 0; }
  .content.markdown-content :global(p:first-child) { margin-top: 0; }
  .content.markdown-content :global(p:last-child) { margin-bottom: 0; }
</style>

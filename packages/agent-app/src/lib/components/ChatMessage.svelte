<script lang="ts">
  import type { Message } from "$lib/types";

  let { message }: { message: Message } = $props();

  const isUser = $derived(message.role === "user");
  const isAssistant = $derived(message.role === "assistant");
  const isSystem = $derived(message.role === "system");
</script>

<div
  class="message"
  class:user={isUser}
  class:assistant={isAssistant}
  class:system={isSystem}
>
  <div class="bubble">
    {#if isSystem}
      <span class="role-label">system</span>
    {:else if isAssistant}
      <span class="role-label">Assistant</span>
    {:else}
      <span class="role-label">You</span>
    {/if}
    <p class="content">{message.content}</p>
  </div>
</div>

<style>
  .message {
    display: flex;
    padding: 4px 16px;
  }

  .message.user {
    justify-content: flex-end;
  }

  .message.assistant,
  .message.system {
    justify-content: flex-start;
  }

  .bubble {
    max-width: 75%;
    padding: 10px 14px;
    border-radius: 12px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
  }

  .message.user .bubble {
    background: var(--color-primary);
    color: var(--color-on-primary);
    border-color: var(--color-primary);
    border-bottom-right-radius: 4px;
  }

  .message.assistant .bubble {
    border-bottom-left-radius: 4px;
  }

  .message.system .bubble {
    background: transparent;
    border: none;
    text-align: center;
    max-width: 100%;
    padding: 6px 14px;
  }

  .role-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    opacity: 0.6;
    display: block;
    margin-bottom: 4px;
  }

  .content {
    margin: 0;
    font-size: 14px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>

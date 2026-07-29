<script lang="ts">
  import ChatMessage from "./ChatMessage.svelte";
  import ChatInput from "./ChatInput.svelte";
  import type { Message } from "$lib/types";

  let {
    messages,
    loading,
    onSend,
  }: {
    messages: Message[];
    loading: boolean;
    onSend: (text: string) => void;
  } = $props();

  let containerEl: HTMLDivElement | undefined = $state();

  // Auto-scroll to bottom when messages change
  $effect(() => {
    if (messages.length > 0 && containerEl) {
      // Small delay to let DOM render
      requestAnimationFrame(() => {
        containerEl!.scrollTop = containerEl!.scrollHeight;
      });
    }
  });
</script>

<div class="chat-area">
  <div class="messages" bind:this={containerEl}>
    {#if messages.length === 0}
      <div class="empty">
        <p>Start a conversation by sending a message below.</p>
      </div>
    {:else}
      {#each messages as msg}
        <ChatMessage message={msg} />
      {/each}
    {/if}

    {#if loading}
      <div class="loading-indicator">
        <span class="dot-pulse"></span>
        <span>Thinking...</span>
      </div>
    {/if}
  </div>

  <ChatInput {onSend} {loading} />
</div>

<style>
  .chat-area {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .messages {
    flex: 1;
    overflow-y: auto;
    padding: 12px 0;
    scroll-behavior: smooth;
  }

  .empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--color-text-muted);
    font-size: 14px;
  }

  .empty p {
    text-align: center;
    max-width: 300px;
  }

  .loading-indicator {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 20px;
    font-size: 13px;
    color: var(--color-text-muted);
  }

  .dot-pulse {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-primary);
    animation: pulse 1.2s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.3; transform: scale(0.8); }
    50% { opacity: 1; transform: scale(1.2); }
  }
</style>

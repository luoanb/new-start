<script lang="ts">
  import ChatMessage from "./ChatMessage.svelte";
  import ChatInput from "./ChatInput.svelte";
  import type { Message } from "$lib/types";
  import { t } from "$lib/i18n";
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import { errorMessage } from "$lib/errorMessage";

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
  let ratingError = $state("");

  $effect(() => {
    if (messages.length > 0 && containerEl) {
      requestAnimationFrame(() => {
        containerEl!.scrollTop = containerEl!.scrollHeight;
      });
    }
  });

  // 仅当当前会话已绑定 topic（assistant 模式）时，assistant 回复才显示评价按钮。
  const canRate = $derived(
    dataStore.state.topics.some(
      (topic) => topic.session_id === dataStore.state.activeConversationId
    )
  );

  async function handleCopy(msg: Message): Promise<void> {
    await navigator.clipboard.writeText(msg.content);
  }

  async function handleRate(score: number): Promise<void> {
    const conversationId = dataStore.state.activeConversationId;
    if (!conversationId) return;
    try {
      await dataStore.scoreFeedback(conversationId, score);
    } catch (e) {
      ratingError = `评价失败: ${errorMessage(e)}`;
      setTimeout(() => (ratingError = ""), 3000);
    }
  }
</script>

<div class="chat-area">
  {#if ratingError}
    <div class="rating-error">{ratingError}</div>
  {/if}
  <div class="messages" bind:this={containerEl}>
    {#if messages.length === 0}
      <div class="empty">
        <div class="empty-content">
          <h3>{t("chatArea.emptyTitle")}</h3>
          <p>{t("chatArea.emptyDesc")}</p>
        </div>
      </div>
    {:else}
      {#each messages as msg}
        <ChatMessage message={msg} {canRate} onCopy={handleCopy} onRate={handleRate} />
      {/each}
    {/if}

    {#if loading}
      <div class="loading-indicator">
        <span class="dot-pulse"></span>
        <span>{t("common.thinking")}</span>
      </div>
    {/if}
  </div>

  <ChatInput {onSend} {loading} />
</div>

<style>
  .chat-area { display: flex; flex-direction: column; height: 100%; overflow: hidden; min-height: 0; background: var(--color-bg); }
  .rating-error { margin: var(--space-1) var(--space-4); padding: var(--space-1) var(--space-2); font-size: var(--fs-xs); color: var(--color-error); background: var(--color-error-bg); border-radius: var(--radius-sm); }
  .messages { flex: 1; overflow-y: auto; min-height: 0; padding: var(--space-3) 0; scroll-behavior: smooth; }
  .empty { display: flex; align-items: center; justify-content: center; height: 100%; }
  .empty-content { text-align: center; max-width: 300px; }
  .empty-content h3 { margin: 0 0 var(--space-2); font-size: var(--fs-lg); font-weight: 600; color: var(--color-text); }
  .empty-content p { margin: 0; font-size: var(--fs-sm); color: var(--color-text-muted); }
  .loading-indicator { display: flex; align-items: center; gap: var(--space-2); padding: var(--space-2) var(--space-5); font-size: var(--fs-sm); color: var(--color-text-muted); }
  .dot-pulse { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--color-primary); animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 0%, 100% { opacity: 0.3; transform: scale(0.8); } 50% { opacity: 1; transform: scale(1.2); } }
</style>

<script lang="ts">
  import ChatMessage from "./ChatMessage.svelte";
  import ChatInput from "./ChatInput.svelte";
  import type { Message } from "$lib/types";
  import { t } from "$lib/i18n";
  import { errorMessage } from "$lib/errorMessage";
  import { useViewContext } from "$lib/layout/viewContext";

  // 视图数据/命令统一来自 ViewContext（容器与内容解耦，无 props）。
  const ctx = useViewContext();
  let messages = $derived(ctx.stores.data.state.messages);
  let providers = $derived(ctx.stores.data.state.providers);
  let models = $derived(ctx.stores.data.state.models);
  let selectedProviderId = $derived(ctx.ui.activeProviderId);
  let selectedModelId = $derived(ctx.ui.activeModelId);

  // 会话级运行状态：单一真相源 = 后端 runningSessions（多会话并行互不影响）。
  // 发送按钮防抖锁 sendingIds 不参与运行状态判定，避免其残留导致永久"思考中"。
  let activeConversationId = $derived(ctx.stores.data.state.activeConversationId ?? "");
  let runningSession = $derived(
    ctx.stores.data.state.runningSessions.find((s) => s.session_id === activeConversationId)
  );
  let isRunning = $derived(!!runningSession);

  const onSend = (text: string) => void ctx.commands.sendMessage(text);
  const onModelChange = (providerId: string, modelId: string) =>
    ctx.commands.changeModel(providerId, modelId);

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
    ctx.stores.data.state.topics.some(
      (topic) => topic.session_id === ctx.stores.data.state.activeConversationId
    )
  );

  async function handleCopy(msg: Message): Promise<void> {
    await navigator.clipboard.writeText(msg.body.content);
  }

  async function handleRate(score: number): Promise<void> {
    const conversationId = ctx.stores.data.state.activeConversationId;
    if (!conversationId) return;
    try {
      await ctx.stores.data.scoreFeedback(conversationId, score);
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

    {#if isRunning}
      <div class="loading-indicator">
        <span class="dot-pulse"></span>
        <span>{t("common.thinking")}</span>
        {#if runningSession?.current_step}
          <span class="running-step">{runningSession.current_step}</span>
        {/if}
      </div>
    {/if}
  </div>

  <ChatInput
    {onSend}
    loading={isRunning}
    {providers}
    {models}
    {selectedProviderId}
    {selectedModelId}
    {onModelChange}
  />
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
  .running-step { font-family: var(--font-mono, monospace); font-size: var(--fs-xs); color: var(--color-primary); opacity: 0.85; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .dot-pulse { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--color-primary); animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 0%, 100% { opacity: 0.3; transform: scale(0.8); } 50% { opacity: 1; transform: scale(1.2); } }
</style>

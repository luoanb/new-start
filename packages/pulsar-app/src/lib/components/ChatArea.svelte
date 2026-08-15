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

  const onSend = (text: string) => {
    pendingAlignTop = true;
    void ctx.commands.sendMessage(text);
  };
  const onModelChange = (providerId: string, modelId: string) =>
    ctx.commands.changeModel(providerId, modelId);

  let containerEl: HTMLDivElement | undefined = $state();
  let ratingError = $state("");
  // 对话容器可视高度（px）：轮次小容器 min-height 的基准。
  // 不能直接用 CSS 百分比——.messages 是滚动容器，子元素 min-height:100%
  // 会因父级高度不确定而无法解析（computed 返回 "100%" 而非像素）。
  let viewportH = $state(0);
  $effect(() => {
    const el = containerEl;
    if (!el) return;
    const ro = new ResizeObserver(() => (viewportH = el.clientHeight));
    ro.observe(el);
    viewportH = el.clientHeight;
    return () => ro.disconnect();
  });

  // ── Gemini 式对话滚动 ──
  // 语义：仅当用户已在底部时才自动跟随新消息；上滑阅读历史时锁定（不被打断），
  // 回到底部解锁；发送问句后把新问题对齐视口顶部，为回答预留空间。
  let userScrolled = $state(false); // true = 用户已上滑离开底部（锁定自动跟随）
  let pendingAlignTop = $state(false); // 发送后待对齐顶部
  let stickyRound = $state(false); // 吸顶展示期：问题吸顶后，回答到达不跳到底（Gemini 形态）
  let autoScrolling = false; // 程序化滚动中：抑制 onscroll 误判为用户上滑
  let lastMessageCount = 0;
  let lastMessageRole = "";

  function handleScroll() {
    const el = containerEl;
    if (!el || autoScrolling) return;
    // 距底剩余像素，5px 容错防亚像素抖动；离开底部即锁定，回到底部解锁。
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    userScrolled = distanceFromBottom > 5;
    // 用户主动滚动即结束吸顶展示期，交还控制权。
    if (userScrolled) stickyRound = false;
  }

  function scrollToNewest() {
    requestAnimationFrame(() => {
      if (containerEl) containerEl.scrollTop = containerEl.scrollHeight;
    });
  }

  /** 把目标消息对齐到滚动容器顶部。临时覆盖容器 CSS 的 scroll-behavior:smooth，
   *  否则 scrollIntoView(block:'start') 会被平滑动画吞掉、只滚到接近底部。 */
  function scrollToTopOf(target: HTMLElement) {
    const el = containerEl;
    if (!el) return;
    const prev = el.style.scrollBehavior;
    el.style.scrollBehavior = "auto";
    target.scrollIntoView({ block: "start" });
    el.style.scrollBehavior = prev;
  }

  $effect(() => {
    const list = messages;
    if (list.length === 0 || !containerEl) return;
    const last = list[list.length - 1];

    // 列表被重置（切换会话/清空历史）：解锁并回到底部。
    if (list.length < lastMessageCount) {
      userScrolled = false;
      stickyRound = false;
      lastMessageCount = 0;
      lastMessageRole = "";
    }

    const isNewMessage = list.length > lastMessageCount;

    if (pendingAlignTop && isNewMessage) {
      // Gemini 行为：发送后把新问题对齐视口顶部，为回答预留空间。
      // 列表末尾的 .answer-spacer 提供底部预留空间，让"最后一条消息"也能
      // 真正吸顶到视口顶部（无预留时受 maxScrollTop 限制会被 clamp 回底部）。
      pendingAlignTop = false;
      stickyRound = true;
      requestAnimationFrame(() => {
        const el = containerEl;
        if (!el) return;
        const items = el.querySelectorAll(".message.user");
        const target = items[items.length - 1] as HTMLElement | undefined;
        if (target) {
          // 发送是用户主动操作，视为回到最新：解锁后续自动跟随。
          userScrolled = false;
          autoScrolling = true;
          scrollToTopOf(target);
          // 下一帧恢复 onscroll 监听，避免本次程序化滚动被判定为用户上滑。
          requestAnimationFrame(() => (autoScrolling = false));
        }
      });
    } else if (isNewMessage && !userScrolled) {
      if (stickyRound) {
        // 吸顶展示期收到新消息（回答到达）：问题已吸顶在视口顶部，回答自然
        // 出现在其下方，无需额外滚动（再滚动会把问题顶出视口、破坏吸顶）。
        // 回答短则问题+回答都在视口内；回答长时用户按需自行滚动。
        // 吸顶展示期到此结束，交还控制权。
        stickyRound = false;
      } else {
        // 其余新消息：仅在用户未上滑时自动跟随到底。
        scrollToNewest();
      }
    }

    lastMessageCount = list.length;
    lastMessageRole = last.role;
  });

  // 评价按钮：会话绑定 topic 时所有 assistant 消息均可评（评分定位所在介入区间，
  // 允许随时评分、重复评分；后端按 message_index 推导区间盖章神经元）。
  const rateable = $derived(
    !!ctx.stores.data.state.topics.some(
      (topic) => topic.session_id === ctx.stores.data.state.activeConversationId
    )
  );

  // ── 轮次分组（纯前端展示层，不改数据）──
  // 一轮对话 = 以用户输入为起点、到下一个用户输入之前（不含）为止的连续消息。
  // nudge 消息 role=user 但 body.kind==="nudge"，是轮内简报，不作为轮起点。
  type MessageRound = { startIndex: number; messages: Message[] };
  const rounds = $derived.by<MessageRound[]>(() => {
    const groups: MessageRound[] = [];
    let current: MessageRound | null = null;
    messages.forEach((msg, i) => {
      const isRoundStart = msg.role === "user" && msg.body.kind !== "nudge";
      if (isRoundStart) {
        current = { startIndex: i, messages: [msg] };
        groups.push(current);
      } else {
        if (!current) {
          // 前导非 user 消息（如被压缩/截断的历史开头）自成一组。
          current = { startIndex: i, messages: [] };
          groups.push(current);
        }
        current.messages.push(msg);
      }
    });
    return groups;
  });

  async function handleCopy(msg: Message): Promise<void> {
    await navigator.clipboard.writeText(msg.body.content);
  }

  async function handleRate(messageIndex: number, score: number): Promise<void> {
    const conversationId = ctx.stores.data.state.activeConversationId;
    if (!conversationId) return;
    try {
      await ctx.stores.data.scoreFeedback(conversationId, messageIndex, score);
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
  <div class="messages" bind:this={containerEl} onscroll={handleScroll}>
    {#if messages.length === 0}
      <div class="empty">
        <div class="empty-content">
          <h3>{t("chatArea.emptyTitle")}</h3>
          <p>{t("chatArea.emptyDesc")}</p>
        </div>
      </div>
    {:else}
      {#each rounds as round, i}
        <div
          class="message-round"
          class:last={i === rounds.length - 1}
          style={i === rounds.length - 1 ? `min-height: ${viewportH}px` : undefined}
        >
          {#each round.messages as msg, mi}
            <ChatMessage
              message={msg}
              canRate={rateable}
              onCopy={handleCopy}
              onRate={(score) => handleRate(round.startIndex + mi, score)}
            />
          {/each}
        </div>
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
  /* 一轮对话的小容器：仅最后一轮（最新）注入 min-height = 对话容器可视高度
     （由 viewportH 内联注入，避免滚动容器内百分比高度无法解析），使最新一轮
     至少占满一屏、天然吸顶（问题在上、回答在下）；历史轮按内容自然高度展示。 */
  .message-round { min-height: 0; }
  .message-round + .message-round { margin-top: var(--space-4); }
  .empty { display: flex; align-items: center; justify-content: center; height: 100%; }
  .empty-content { text-align: center; max-width: 300px; }
  .empty-content h3 { margin: 0 0 var(--space-2); font-size: var(--fs-lg); font-weight: 600; color: var(--color-text); }
  .empty-content p { margin: 0; font-size: var(--fs-sm); color: var(--color-text-muted); }
  .loading-indicator { display: flex; align-items: center; gap: var(--space-2); padding: var(--space-2) var(--space-5); font-size: var(--fs-sm); color: var(--color-text-muted); }
  .running-step { font-family: var(--font-mono, monospace); font-size: var(--fs-xs); color: var(--color-primary); opacity: 0.85; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .dot-pulse { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--color-primary); animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 0%, 100% { opacity: 0.3; transform: scale(0.8); } 50% { opacity: 1; transform: scale(1.2); } }
</style>

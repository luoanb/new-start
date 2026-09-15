<script lang="ts">
  import ChatMessage from "./ChatMessage.svelte";
  import JudgementCard from "./JudgementCard.svelte";
  import ChatInput from "./ChatInput.svelte";
  import type { Message, SamplingParams, ThinkingConfig } from "$lib/types";
  import type { HookDefMeta, HookJudgementRecord } from "$lib/types";
  import { t } from "$lib/i18n";
  import { errorMessage } from "$lib/errorMessage";
  import { CopyToClipboard } from "$lib/utils";
  import { useViewContext } from "$lib/layout/viewContext";
  import type { MainPanel } from "$lib/layout/layoutTypes";
  import { api, c } from "$lib/api";
  import { getContext, onDestroy, onMount } from "svelte";

  // 视图数据/命令统一来自 ViewContext（容器与内容解耦，无 props）。
  const ctx = useViewContext();

  // 面板实例（ViewHost 注入）：绑定会话窗口的 panel.id = `chat:${conversationId}`；
  // 无 `chat:` 前缀 = 主窗口（跟随全局激活会话）。
  const panel = getContext<MainPanel | undefined>("pulsar:panel");
  const boundConversationId = $derived(
    panel?.id.startsWith("chat:") ? panel.id.slice("chat:".length) : null,
  );
  // 本窗口显示会话：绑定窗口固定绑定，主窗口跟随全局激活。
  let activeConversationId = $derived(ctx.stores.data.state.activeConversationId ?? "");
  let conversationId = $derived(boundConversationId ?? activeConversationId);
  // 会话消息视图（主/绑定窗口共享同一份 chatViews[conversationId] 缓存）。
  let view = $derived(ctx.stores.data.chatViews[conversationId]);
  let messages = $derived(view?.messages ?? []);
  // 分页窗口：messagesOffset = 首条已加载消息在整段历史中的绝对下标
  // （评分/裁决卡锚点/定位均消费绝对下标；streamingIndex 保持窗口内下标语义）。
  let messagesOffset = $derived(view?.offset ?? 0);
  let messagesHasMore = $derived(view?.hasMore ?? false);
  let messagesLoadingOlder = $derived(view?.loadingOlder ?? false);
  let providers = $derived(ctx.stores.data.state.providers);
  let models = $derived(ctx.stores.data.state.models);
  let selectedProviderId = $derived(ctx.ui.activeProviderId);
  let selectedModelId = $derived(ctx.ui.activeModelId);
  let selectedParams = $derived(ctx.ui.activeParams);
  let selectedThinking = $derived(ctx.ui.activeThinking);

  // 会话级运行状态：单一真相源 = 后端 runningSessions（多会话并行互不影响）。
  // 发送按钮防抖锁 sendingIds 不参与运行状态判定，避免其残留导致永久"思考中"。
  let runningSession = $derived(
    ctx.stores.data.state.runningSessions.find((s) => s.session_id === conversationId)
  );
  let isRunning = $derived(!!runningSession);

  // 介入轮「进行中」的兜底信号：最后一条真实用户输入（role=user 且文本；nudge/role_context 不算）。
  // 仅用于后端进行中标记（elapsed_ms=0）落库前的首轮窗口；标记落库后不再依赖运行态。
  const lastUserInputIndex = $derived(
    (() => {
      for (let i = messages.length - 1; i >= 0; i--) {
        const msg = messages[i];
        if (msg.role === "user" && msg.body.kind === "text") return i;
      }
      return -1;
    })()
  );

  const onSend = (text: string) => {
    pendingAlignTop = true;
    if (boundConversationId) {
      void ctx.commands.sendMessageTo(boundConversationId, text);
    } else {
      void ctx.commands.sendMessage(text);
    }
  };
  const onStop = () => {
    if (conversationId) void ctx.commands.stopRunningSession(conversationId);
  };
  const onModelChange = (
    providerId: string,
    modelId: string,
    params?: SamplingParams,
    thinking?: ThinkingConfig,
  ) => ctx.commands.changeModel(providerId, modelId, params, thinking);

  let containerEl: HTMLDivElement | undefined = $state();
  let ratingError = $state("");
  // 对话容器可视高度（px）：轮次小容器 min-height 的基准。
  // 不能直接用 CSS 百分比——.messages 是滚动容器，子元素 min-height:100%
  // 会因父级高度不确定而无法解析（computed 返回 "100%" 而非像素）。
  // 减去 VIEWPORT_OFFSET：底部留 12px 呼吸空隙，避免最后一条内容贴死底边。
  let viewportH = $state(0);
  const VIEWPORT_OFFSET = 16;
  $effect(() => {
    const el = containerEl;
    if (!el) return;
    const ro = new ResizeObserver(() => (viewportH = el.clientHeight - VIEWPORT_OFFSET));
    ro.observe(el);
    viewportH = el.clientHeight - VIEWPORT_OFFSET;
    return () => ro.disconnect();
  });

  // ── 对话滚动 ──
  // 语义：只在用户主动发送时滚动（把新问题对齐视口顶部）；模型回复 / 工具推进一律
  // 不动视口，避免多轮推进时反复把视图拽到底部打断阅读。
  let pendingAlignTop = $state(false); // 发送后待对齐顶部
  let autoScrolling = false; // 程序化滚动中：抑制 onscroll 误判
  let lastMessageCount = 0;
  let lastMessageRole = "";

  function handleScroll() {
    const el = containerEl;
    if (!el || autoScrolling) return;
    // 上滑近顶部：追加加载更早消息（滚动位置由 loadOlderMessages 按高度差恢复）。
    if (el.scrollTop <= 40) {
      void loadOlderMessages();
    }
  }

  /**
   * 加载更早消息（前插到窗口头部）。前插前记录容器高度与 scrollTop，
   * 前插后以高度差补偿 scrollTop，保持用户当前阅读位置不跳变。
   */
  async function loadOlderMessages(): Promise<void> {
    const el = containerEl;
    if (!el || !messagesHasMore || messagesLoadingOlder) return;
    const prevScrollHeight = el.scrollHeight;
    const prevScrollTop = el.scrollTop;
    await ctx.stores.data.loadMoreMessages(conversationId);
    // 等待 DOM 按新数组完成一轮渲染后再补偿滚动位置。
    requestAnimationFrame(() => {
      if (containerEl) {
        containerEl.scrollTop = containerEl.scrollHeight - prevScrollHeight + prevScrollTop;
      }
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

    // 列表被重置（切换会话/清空历史）：重置计数基线。
    if (list.length < lastMessageCount) {
      lastMessageCount = 0;
      lastMessageRole = "";
    }

    if (pendingAlignTop && list.length > lastMessageCount) {
      // 仅用户发送：把新问题对齐视口顶部，为回答预留空间（轮次容器自身等高提供
      // 底部空间，故可真正吸顶）。回答 / 工具消息到达一律不滚动。
      pendingAlignTop = false;
      requestAnimationFrame(() => {
        const el = containerEl;
        if (!el) return;
        const items = el.querySelectorAll(".message.user");
        const target = items[items.length - 1] as HTMLElement | undefined;
        if (target) {
          autoScrolling = true;
          scrollToTopOf(target);
          // 下一帧恢复 onscroll 监听，避免本次程序化滚动触发分页判定。
          requestAnimationFrame(() => (autoScrolling = false));
        }
      });
    }

    lastMessageCount = list.length;
    lastMessageRole = last.role;
  });

  // 评价按钮：会话绑定 topic 时所有 assistant 消息均可评（评分定位所在介入区间，
  // 允许随时评分、重复评分；后端按 message_index 推导区间盖章神经元）。
  const rateable = $derived(
    !!ctx.stores.data.state.topics.some((topic) => topic.session_id === conversationId)
  );

  // ── 轮次分组（纯前端展示层，不改数据）──
  // 一轮对话 = 以用户输入为起点、到下一个用户输入之前（不含）为止的连续消息。
  // nudge 消息 role=user 但 body.kind==="nudge"，是轮内简报，不作为轮起点；
  // role_context 消息 role=user 但 body.kind==="role_context"，是 B2 角色切换（审计/展示），也不作为轮起点。
  type MessageRound = { startIndex: number; messages: Message[] };
  const rounds = $derived.by<MessageRound[]>(() => {
    const groups: MessageRound[] = [];
    let current: MessageRound | null = null;
    messages.forEach((msg, i) => {
      const isRoundStart =
        msg.role === "user" &&
        msg.body.kind !== "nudge" &&
        msg.body.kind !== "role_context";
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

  // ── 介入轮耗时（展示在分组底部）──
  // `elapsed_ms` 三态：0 = 后端标记「进行中」→ 按起点本地 tick；>0 = 已收尾定格；缺失 = 未追踪。
  let nowMs = $state(Date.now());

  /** 该轮（以用户输入为起点）是否处于「进行中」。 */
  const roundTurnOpen = (round: MessageRound): boolean => {
    const anchor = round.messages[0];
    if (!anchor || anchor.role !== "user" || anchor.body.kind !== "text") return false;
    if (anchor.elapsed_ms === 0) return true;
    // 进行中标记落库前的首轮窗口：运行中 + 最后一条用户输入。
    return anchor.elapsed_ms == null && isRunning && round.startIndex === lastUserInputIndex;
  };
  const hasOpenTurn = $derived(rounds.some(roundTurnOpen));
  $effect(() => {
    if (!hasOpenTurn) return;
    nowMs = Date.now();
    const timer = setInterval(() => {
      nowMs = Date.now();
    }, 1000);
    return () => clearInterval(timer);
  });

  /** 墙钟时长格式化：`45s` / `2m13s` / `1h05m`。 */
  function formatDuration(ms: number): string {
    const totalSec = Math.max(0, Math.floor(ms / 1000));
    const h = Math.floor(totalSec / 3600);
    const m = Math.floor((totalSec % 3600) / 60);
    const s = totalSec % 60;
    if (h > 0) return `${h}h${m.toString().padStart(2, "0")}m`;
    if (m > 0) return `${m}m${s.toString().padStart(2, "0")}s`;
    return `${s}s`;
  }

  /** 本轮介入轮耗时的展示文本（`null` = 不展示）。 */
  function formatRoundElapsed(round: MessageRound): string | null {
    const anchor = round.messages[0];
    if (!anchor || anchor.role !== "user" || anchor.body.kind !== "text") return null;
    const settled = anchor.elapsed_ms;
    if (typeof settled === "number" && settled > 0) return formatDuration(settled);
    if (roundTurnOpen(round)) return formatDuration(Math.max(0, nowMs - anchor.timestamp));
    return null;
  }

  /** 末尾轮耗时（仅末轮仍在进行中）：透传给输入框，固定在发送按钮左侧实时递增展示；
   *  无轮次 / 末轮已收尾时为 null，不展示（收尾耗时已在消息区轮次分组底部定格展示）。 */
  const lastRoundElapsed = $derived.by<string | null>(() => {
    const last = rounds[rounds.length - 1];
    if (!last || !roundTurnOpen(last)) return null;
    return formatDuration(Math.max(0, nowMs - last.messages[0].timestamp));
  });

  async function handleCopy(msg: Message): Promise<boolean> {
    return CopyToClipboard.copyText(msg.body.content);
  }

  async function handleRate(messageIndex: number, score: number): Promise<void> {
    const cid = conversationId;
    if (!cid) return;
    try {
      await ctx.stores.data.scoreFeedback(cid, messageIndex, score);
    } catch (e) {
      ratingError = `评价失败: ${errorMessage(e)}`;
      setTimeout(() => (ratingError = ""), 3000);
    }
  }

  // ── 锚点定位：面板「在会话中定位」→ 滚动高亮锚点消息 ──
  // 会话切换后消息异步加载，目标元素可能未就绪；锚点消息若不在已加载窗口
  // （分页前插后仍未覆盖），自动续拉更早页直到命中或拉完。
  $effect(() => {
    const anchor = ctx.stores.layout.locateAnchor;
    if (!anchor || anchor.conversationId !== conversationId) return;
    ctx.stores.layout.clearLocate();
    void locateToMessage(anchor.messageIndex);
  });

  async function locateToMessage(messageIndex: number): Promise<void> {
    for (let i = 0; i < 20; i++) {
      // 等待一轮渲染（初次查询/前插后 DOM 对齐），避免命中已加载但未渲染的消息。
      await new Promise((r) => requestAnimationFrame(r));
      const target = containerEl?.querySelector(
        `[data-message-index="${messageIndex}"]`,
      ) as HTMLElement | undefined;
      if (target) {
        scrollToTopOf(target);
        target.classList.add("locate-flash");
        setTimeout(() => target.classList.remove("locate-flash"), 2200);
        return;
      }
      // 目标不在窗口内：续拉更早页（全部拉完仍无 → 静默放弃）。
      if (!messagesHasMore) return;
      await loadOlderMessages();
    }
  }

  // ── 消息内联裁决卡：锚点附属渲染块（旁路列表，不插入消息数组）──
  let judgements = $state<HookJudgementRecord[]>([]);
  let hookDefs = $state<HookDefMeta[]>([]);
  let unlistenJudgements: (() => void) | null = null;

  /** 拉取当前会话的裁决记录（按 conversationId 过滤，后端倒序）。 */
  async function refreshJudgements() {
    if (!conversationId) {
      judgements = [];
      return;
    }
    try {
      const [list, defs] = await Promise.all([
        api.call(c.hookJudgementsList, {
          filters: { conversationId },
        }),
        api.call(c.hookDefsList, undefined),
      ]);
      judgements = list.records;
      hookDefs = defs;
    } catch {
      // 裁决卡为附属展示，拉取失败静默降级（不影响主消息渲染）。
      judgements = [];
    }
  }

  // 会话切换（含首次挂载）时重拉；事件驱动后续实时刷新。
  $effect(() => {
    void refreshJudgements();
  });

  onMount(() => {
    unlistenJudgements = api.subscribe((payload) => {
      if (payload.kind === "hook_judgements" && payload.conversation_id === conversationId) {
        void refreshJudgements();
      }
    });
  });

  onDestroy(() => {
    unlistenJudgements?.();
    unlistenJudgements = null;
  });

  /** 某条消息索引关联的裁决记录（同一锚点可能挂载多个 hook 裁决，全量渲染）。 */
  function judgementsFor(messageIndex: number): HookJudgementRecord[] {
    return judgements.filter((j) => j.anchor_message_index === messageIndex);
  }

  /** hook 展示名（label 是 i18n key；未知类型回退 system_type 原文）。 */
  function hookLabelFor(record: HookJudgementRecord): string {
    const def = hookDefs.find((d) => d.system_type === record.hook_type);
    return def ? t(def.label) : record.hook_type;
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
        {@const turnElapsed = formatRoundElapsed(round)}
        <div
          class="message-round"
          class:last={i === rounds.length - 1}
          style={i === rounds.length - 1 ? `min-height: ${viewportH}px` : undefined}
        >
          {#each round.messages as msg, mi}
            {@const absIndex = messagesOffset + round.startIndex + mi}
            <ChatMessage
              message={msg}
              // 紧邻上一条工具回复时压缩纵向间距，让一轮内的多条工具结果更像连续列表
              compactTool={mi > 0 && round.messages[mi - 1].body.kind === "tool_result"}
              streaming={(view?.streamingIndex ?? null) === round.startIndex + mi}
              canRate={rateable}
              anchorIndex={absIndex}
              onCopy={handleCopy}
              onRate={(score) => handleRate(absIndex, score)}
            />
            {#each judgementsFor(absIndex) as record (record.id)}
              <!-- 裁决卡：锚点消息附属渲染块（旁路列表，不插入消息数组、不影响 message_index） -->
              <JudgementCard {record} hookLabel={hookLabelFor(record)} />
            {/each}
          {/each}
          {#if isRunning && i === rounds.length - 1}
            <div class="loading-indicator">
              <span class="dot-pulse"></span>
              <span>{t("common.thinking")}</span>
              {#if runningSession?.current_step}
                <span class="running-step">{runningSession.current_step}</span>
              {/if}
            </div>
          {/if}
          {#if turnElapsed !== null}
            <div class="turn-elapsed">{t("chatMessage.turnElapsed", { duration: turnElapsed })}</div>
          {/if}
        </div>
      {/each}
    {/if}

    {#if isRunning && rounds.length === 0}
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
    running={isRunning}
    {onStop}
    {providers}
    {models}
    {selectedProviderId}
    {selectedModelId}
    params={selectedParams}
    thinking={selectedThinking}
    turnElapsed={lastRoundElapsed}
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
  /* 介入轮耗时：分组底部的汇总脚注（与消息内容左边缘对齐，弱化展示）。 */
  .turn-elapsed { padding: var(--space-1) var(--space-4) 0; font-size: var(--fs-xs); color: var(--color-text-muted); opacity: 0.7; }
  .running-step { font-family: var(--font-mono, monospace); font-size: var(--fs-xs); color: var(--color-primary); opacity: 0.85; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .dot-pulse { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--color-primary); animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 0%, 100% { opacity: 0.3; transform: scale(0.8); } 50% { opacity: 1; transform: scale(1.2); } }
  /* 锚点定位高亮：面板「在会话中定位」滚动后给目标消息短暂描边（ChatArea JS 增删类）。
     keyframes 在组件作用域内定义，Svelte 编译时统一哈希并替换 :global 内的 animation 引用。 */
  :global(.message.locate-flash) {
    animation: locate-flash 2.2s ease;
  }
  @keyframes locate-flash {
    0%, 100% { box-shadow: none; }
    12%, 48% { box-shadow: 0 0 0 2px var(--color-primary), 0 0 14px var(--color-primary); }
  }
</style>

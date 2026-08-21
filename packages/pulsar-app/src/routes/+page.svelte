<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import SessionCreateModal from "$lib/components/SessionCreateModal.svelte";
  import ConnectDialog from "$lib/components/ConnectDialog.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import ErrorBanner from "$lib/components/ErrorBanner.svelte";
  import ActivityBar from "$lib/layout/ActivityBar.svelte";
  import Splitter from "$lib/layout/Splitter.svelte";
  import WindowEdgeResize from "$lib/layout/WindowEdgeResize.svelte";
  import EditorTabs from "$lib/layout/EditorTabs.svelte";
  import ViewHost from "$lib/layout/ViewHost.svelte";
  import ViewContainer from "$lib/layout/ViewContainer.svelte";
  import { layoutStore } from "$lib/layout/LayoutStore.svelte";
  import { applyThemeOnBoot } from "$lib/theme";
  import { mainViews, mainPanelMeta } from "$lib/layout/views";
  import { setViewContext, type ViewContext } from "$lib/layout/viewContext";
  import { fileEditorStore } from "$lib/stores/fileEditorStore.svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import GitConfirmHost from "$lib/components/GitConfirmHost.svelte";
  import { t } from "$lib/i18n";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import { hotkeyService } from "$lib/hotkey/hotkeyService";
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import { discoverRemote, isTauriEnv, switchConn } from "$lib/api";
  import type { SamplingParams, ThinkingConfig, ChatModelSelection } from "$lib/types";

  // ── 统一数据（dataStore 驱动：bootstrap + 事件订阅刷新）──
  let runtimeStatus = $derived(dataStore.state.runtimeStatus);
  let ready = $derived(dataStore.state.ready);

  // 读取 localStorage 选择项并一次性迁移旧键（agent-app:* → pulsar:*）
  function persistedSelection(key: string, legacyKey: string): string {
    const value = localStorage.getItem(key);
    if (value === null) {
      const legacy = localStorage.getItem(legacyKey);
      if (legacy !== null) {
        localStorage.setItem(key, legacy);
        localStorage.removeItem(legacyKey);
        return legacy;
      }
    }
    return value ?? "";
  }

  // ── ViewContext：视图共享的会话级 UI 状态（$state 保证响应式传播）──
  // 运行状态由 dataStore.runningSessions 权威驱动（后端多会话并行）；
  // sendingIds 仅做本会话发送请求的瞬时防连点，互不阻塞其他会话。
  let ui = $state({
    activeProviderId: persistedSelection("pulsar:providerId", "agent-app:providerId"),
    activeModelId: persistedSelection("pulsar:modelId", "agent-app:modelId"),
    activeParams: undefined as SamplingParams | undefined,
    activeThinking: undefined as ThinkingConfig | undefined,
    sendingIds: new Set<string>(),
  });

  // ── UI state ──
  let error = $state("");
  let showCreateModal = $state(false);
  let showConnectDialog = $state(false);
  let showSettingsDialog = $state(false);
  // 非 Tauri 环境为纯远程访问：连接失败时自动弹出连接弹窗，并锁定不可关闭（直到连接成功）。
  let remoteConnLocked = $state(false);
  let drawerSidebar = $state(false);
  let drawerInfo = $state(false);
  // 移动端底栏：panel 容器以底部抽屉（drawer-bottom）展示
  let drawerPanel = $state(false);
  // 小屏导航栏（ActivityBar）显隐：默认隐藏，点击顶栏 logo 切换
  let activityOpen = $state(false);
  // 关闭未保存文件 tab 前的确认请求（ConfirmDialog 消费后置 null）
  let confirmReq = $state<{
    title: string;
    message: string;
    confirmLabel?: string;
    danger?: boolean;
    onConfirm: () => void;
  } | null>(null);

  // ── Layout (store-driven) ──
  let mainRef = $state<HTMLElement | null>(null);
  // main 区面板：用户交互插入/关闭，默认空；同一类型全局唯一
  let mainPanes = $derived(layoutStore.state.main.panes);
  let activePaneId = $derived(layoutStore.state.main.activePaneId);
  // 当前激活分栏（用于 ActivityBar 高亮判定）
  let activePane = $derived(mainPanes.find((p) => p.id === activePaneId));
  // ActivityBar 高亮：chat 面板激活时点亮对话入口，否则跟随侧栏活动
  let activityBarActive = $derived(
    activePane?.panels.some((x) => x.id === activePane.activePanelId && x.type === "chat")
      ? "chat"
      : layoutStore.state.activity.active
  );
  let sidebarStyle = $derived(
    layoutStore.state.sidebar.visible
      ? `width:${layoutStore.state.sidebar.width}px`
      : "width:0"
  );
  let infoStyle = $derived(
    layoutStore.state.info.visible
      ? `width:${layoutStore.state.info.width}px`
      : "width:0"
  );
  let panelStyle = $derived(
    layoutStore.state.panel.visible
      ? `height:${layoutStore.state.panel.height}px`
      : "height:0"
  );

  // ── Derived ──
  let hasModel = $derived(!!ui.activeProviderId && !!ui.activeModelId);

  // 非 Tauri 环境（纯远程访问）：一旦出现连接错误即自动弹出连接弹窗并锁定，
  // 直到用户在弹窗内成功保存并切换到可用连接（save 成功后触发 onClose 解锁）。
  $effect(() => {
    if (!isTauriEnv && dataStore.state.error && !remoteConnLocked) {
      remoteConnLocked = true;
      showConnectDialog = true;
    }
  });

  // ── Bootstrap：统一拉取 + 订阅后端状态事件 ──
  onMount(async () => {
    // 启动即应用已保存的主题偏好，避免等到打开设置弹窗（ThemeSwitcher 挂载）才生效造成跳变。
    applyThemeOnBoot();
    // 非 Tauri 环境：未显式配置远程地址时做同源自动发现——页面若由 pulsar-server 托管
    // （GET /config 可达），直接采用当前 origin，前端无需知道端口。
    if (!isTauriEnv) {
      const discovered = await discoverRemote();
      if (discovered) switchConn({ mode: "remote", url: discovered });
    }
    await dataStore.bootstrap();
    await dataStore.subscribe();
    // 首启默认会话回显后端持有的会话级模型选择（后端权威）。
    echoSessionModel(dataStore.state.activeConversationId);
    setupHotkeys();
    void setWindowIcon();
  });

  /** 运行时设置窗口图标（Linux 桌面导航栏/任务栏显示）。
   * 打包安装后由 .desktop 图标接管；dev 模式与未打包场景依赖此调用生效。 */
  async function setWindowIcon() {
    try {
      const res = await fetch("/favicon.png");
      if (!res.ok) return;
      const bytes = new Uint8Array(await res.arrayBuffer());
      await getCurrentWindow().setIcon(bytes);
    } catch {
      // 非 Tauri 环境或失败时静默，不影响应用启动
    }
  }

  async function handleSend(text: string) {
    // 发送目标在发起时固定：期间切换会话不得改变校验/清理目标，
    // 否则 sendingIds 会残留错误会话（该会话永久显示"思考中"且无法发送）。
    const conversationId = dataStore.state.activeConversationId;
    if (!conversationId) {
      error = "No active session. Create a new session first.";
      return;
    }
    if (!hasModel) {
      error = "Select a provider and model before sending.";
      return;
    }
    // sendingIds 仅为发送按钮防抖锁：运行状态由后端 runningSessions 权威驱动，
    // 此处拦截后端 register 事件回来前的同会话连点重复发送。
    if (ui.sendingIds.has(conversationId)) return;
    error = "";
    ui.sendingIds = new Set(ui.sendingIds).add(conversationId);
    try {
      await dataStore.sendMessage(
        text,
        ui.activeProviderId,
        ui.activeModelId,
        ui.activeParams,
        ui.activeThinking,
      );
    } catch (e) {
      error = `Send failed: ${formatInvokeError(e)}`;
    } finally {
      const next = new Set(ui.sendingIds);
      next.delete(conversationId);
      ui.sendingIds = next;
    }
  }

  async function handleCreateSession(mode: string) {
    showCreateModal = false;
    try {
      const id = await dataStore.createConversation(mode);
      echoSessionModel(id);
    } catch (e) {
      error = `Failed to create session: ${formatInvokeError(e)}`;
    }
  }

  async function handleCloseSession(sessionId: string) {
    try {
      // dataStore 内部处理 active 回退与列表/消息刷新。
      await dataStore.closeSession(sessionId);
    } catch (e) {
      error = `Failed to close session: ${formatInvokeError(e)}`;
    }
  }

  /** 中断运行中的会话（后端 close_session 触发 abort 回调）。 */
  async function handleStopSession(sessionId: string) {
    try {
      await dataStore.stopRunningSession(sessionId);
    } catch (e) {
      error = `Failed to stop session: ${formatInvokeError(e)}`;
    }
  }

  function handleSelectConversation(id: string) {
    void dataStore.selectConversation(id);
    // 会话切换：回显后端持有的该会话模型选择（后端权威），未指定则保持现状。
    echoSessionModel(id);
    // 保证会话面板存在（同一类型全局唯一，已存在则激活）
    layoutStore.insertPanel("chat");
    drawerSidebar = false;
  }

  /** 从后端会话 state.model 回显模型选择到 ui（后端权威）；切换会话/创建后调用。 */
  function echoSessionModel(conversationId: string | null) {
    const conv = dataStore.state.conversations.find((c) => c.id === conversationId);
    const model = conv?.extra?.session?.state?.model;
    if (!model) return;
    ui.activeProviderId = model.provider_id;
    ui.activeModelId = model.model_id;
    ui.activeParams = model.params ?? undefined;
    ui.activeThinking = model.thinking ?? undefined;
    localStorage.setItem("pulsar:providerId", model.provider_id);
    localStorage.setItem("pulsar:modelId", model.model_id);
  }

  function handleModelChange(
    providerId: string,
    modelId: string,
    params?: SamplingParams,
    thinking?: ThinkingConfig,
  ) {
    ui.activeProviderId = providerId;
    ui.activeModelId = modelId;
    ui.activeParams = params;
    ui.activeThinking = thinking;
    localStorage.setItem("pulsar:providerId", providerId);
    localStorage.setItem("pulsar:modelId", modelId);
    // 后端持有会话级模型选择：改选即落库（存在激活会话时）。
    const conversationId = dataStore.state.activeConversationId;
    if (conversationId) {
      const selection: ChatModelSelection = {
        provider_id: providerId,
        model_id: modelId,
        params,
        thinking,
      };
      void dataStore.setSessionModel(conversationId, selection).catch((e) => {
        error = `Failed to persist model: ${formatInvokeError(e)}`;
      });
    }
  }

  // ── ViewContext：容器与内容解耦的边界（容器只消费注册表，视图组件自取 context）──
  function viewForType(type: string) {
    return mainViews.find((v) => v.id === type)!;
  }

  // 工具配置编辑面板：插入/激活 tool-editor 面板（默认第 0 栏，同一类型全局唯一）
  function openToolEditor() {
    layoutStore.insertPanel("tool-editor");
  }
  function closeToolEditor() {
    const panel = mainPanes.flatMap((p) => p.panels).find((x) => x.type === "tool-editor");
    if (panel) layoutStore.closePanel(panel.id);
  }
  function closeProviderManager() {
    const panel = mainPanes.flatMap((p) => p.panels).find((x) => x.type === "provider-manager");
    if (panel) layoutStore.closePanel(panel.id);
  }

  const viewCtx: ViewContext = {
    stores: { data: dataStore, layout: layoutStore },
    ui,
    commands: {
      sendMessage: handleSend,
      stopRunningSession: handleStopSession,
      selectConversation: handleSelectConversation,
      createSession: handleCreateSession,
      closeSession: handleCloseSession,
      changeModel: handleModelChange,
      openCreateModal: () => {
        showCreateModal = true;
        drawerSidebar = false;
        drawerInfo = false;
        drawerPanel = false;
      },
      showError: (msg) => (error = msg),
      dismissError: () => (error = ""),
      openToolEditor,
      closeToolEditor,
      closeProviderManager,
    },
  };
  setViewContext(viewCtx);

  // ── Activity Bar / 布局操作 ──

  /** 神经元面板：只注册一个（默认创建到第 2 栏），再次点击关闭。 */
  function toggleNeuronPanel() {
    const panel = mainPanes.flatMap((p) => p.panels).find((x) => x.type === "neurons");
    if (panel) layoutStore.closePanel(panel.id);
    else layoutStore.insertPanel("neurons", 2);
  }

  function handleActivitySelect(id: string) {
    const active = layoutStore.state.activity.active;
    if (id === "sessions") {
      if (active === id && layoutStore.state.sidebar.visible) { layoutStore.toggleSidebar(); return; }
      layoutStore.setActivity("sessions");
      if (!layoutStore.state.sidebar.visible) layoutStore.toggleSidebar();
      return;
    }
    if (id === "info") {
      if (active === id && layoutStore.state.info.visible) { layoutStore.toggleInfo(); return; }
      layoutStore.setActivity("info");
      if (!layoutStore.state.info.visible) layoutStore.toggleInfo();
      return;
    }
    if (id === "chat") { layoutStore.insertPanel("chat"); return; }
    if (id === "neurons") { toggleNeuronPanel(); return; }
  }

  /** 分栏内 tab ✕ 关闭：关闭对应面板（分栏空则自动收缩）。
   * 文件编辑器未保存时先弹确认；确认后释放编辑器实例元数据。 */
  function handleTabClose(panelId: string) {
    const panel = mainPanes.flatMap((p) => p.panels).find((x) => x.id === panelId);
    if (panel?.type === "file-editor" && fileEditorStore.isDirty(panelId)) {
      confirmReq = {
        title: t("fileEditor.closeUnsavedTitle"),
        message: t("fileEditor.closeUnsavedBody"),
        confirmLabel: t("fileEditor.discard"),
        danger: true,
        onConfirm: () => {
          layoutStore.closePanel(panelId);
          fileEditorStore.dispose(panelId);
        },
      };
      return;
    }
    layoutStore.closePanel(panelId);
    fileEditorStore.dispose(panelId);
  }

  /** 对话标题：复用会话侧栏规则（首条 user/assistant 文本消息，无则占位）。 */
  let activeConversationTitle = $derived.by(() => {
    const conv = dataStore.state.conversations.find(
      (c) => c.id === dataStore.state.activeConversationId,
    );
    if (!conv) return t("views.chat");
    const textMsg = conv.messages.find(
      (m) => m.body.kind === "text" && (m.role === "user" || m.role === "assistant"),
    );
    const content = textMsg?.body.kind === "text" ? textMsg.body.content.trim() : "";
    return content || t("sessionList.newSession");
  });

  /** 当前对话模式：chat tab 的 icon 跟随（字母 + 色调）。 */
  let activeConversationMode = $derived(
    dataStore.state.conversations.find(
      (c) => c.id === dataStore.state.activeConversationId,
    )?.mode ?? "chat",
  );

  /** 分栏内 tab 列表：由该分栏的面板动态生成。 */
  function paneTabs(pane: (typeof mainPanes)[number]) {
    return pane.panels.map((p) => {
      const isFile = p.type === "file-editor";
      const isGitDiff = p.type === "git-diff";
      // git-diff 实例 key = `git-diff:${repoId}:${relPath}`；title = 文件名，tooltip = 相对路径
      const gitRelPath = isGitDiff ? p.id.slice("git-diff:".length).split(":").slice(1).join(":") : "";
      const gitTitle = isGitDiff ? (gitRelPath.split("/").pop() || gitRelPath) : undefined;
      return {
        id: p.id,
        label: mainPanelMeta[p.type].label,
        // 文字 icon：对话 tab 取当前对话模式首字母，其余取面板类型首字母
        icon: (p.type === "chat" ? activeConversationMode : p.type).charAt(0).toUpperCase(),
        // 对话 tab：色调跟随对话模式（对齐会话列表 mode-badge 色板）
        iconTone: p.type === "chat" ? activeConversationMode : undefined,
        // 对话 tab：展示对话标题（原始文本，截断显示）；文件 tab：展示文件名 + 未保存 ●
        title:
          p.type === "chat"
            ? activeConversationTitle
            : isFile
              ? fileEditorStore.titleOf(p.id)
              : isGitDiff
                ? gitTitle
                : undefined,
        truncate: p.type === "chat",
        dirty: isFile ? fileEditorStore.isDirty(p.id) : false,
        tooltip: isFile ? fileEditorStore.pathOf(p.id) : isGitDiff ? gitRelPath : undefined,
      };
    });
  }

  /** 拖拽第 i 个分栏分隔条：调整相邻两栏 grow 权重。 */
  function handlePaneResize(i: number, delta: number) {
    const containerW = mainRef?.clientWidth ?? 800;
    const left = mainPanes[i];
    const right = mainPanes[i + 1];
    if (!left || !right) return;
    const total = left.grow + right.grow;
    const leftPx = containerW * (left.grow / total);
    const ratio = Math.max(-0.9, Math.min(0.9, delta / Math.max(1, leftPx)));
    const newLeftGrow = left.grow * (1 + ratio);
    const newRightGrow = Math.max(0.2, total - newLeftGrow);
    layoutStore.setPaneGrow(left.id, newLeftGrow, false);
    layoutStore.setPaneGrow(right.id, newRightGrow, false);
  }

  /** 拖拽经过分栏（内容区/空白区）时允许放置，作为跨分栏移动的落点。 */
  function handlePaneDragOver(e: DragEvent) {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  }

  /** 拖拽到分栏内容区：追加面板到该分栏（tab 栏上的放置由 EditorTabs 拦截处理）。 */
  function handlePaneDrop(e: DragEvent, paneId: string) {
    e.preventDefault();
    const panelId = e.dataTransfer?.getData("text/plain");
    if (!panelId) return;
    const pane = mainPanes.find((p) => p.id === paneId);
    if (!pane) return;
    layoutStore.movePanel(panelId, paneId, pane.panels.length);
  }

  // ── 快捷键服务（单例）──
  // 初始化时一次性约定绑定的 DOM 根 + 忽略规则；运行时仅注册 combo + 回调。
  // 未命中任何 combo 时服务不 preventDefault，系统/浏览器快捷键（Ctrl+T/W/R 等）恢复正常。
  function setupHotkeys() {
    hotkeyService.initHotkeyService({
      bindRoot: document.body, // 覆盖全局含 drawer
      ignoreInput: true, // 可输入区内按键默认放行
    });

    // 原硬代码快捷键迁移为声明式注册（绑定到全局根）
    hotkeyService.registerHotkey({ key: "j", ctrl: true, shift: true }, () =>
      layoutStore.togglePanel()
    );
    hotkeyService.registerHotkey({ key: "j", ctrl: true }, () => {
      showCreateModal = true;
    });
    hotkeyService.registerHotkey({ key: "b", ctrl: true }, () =>
      layoutStore.toggleSidebar()
    );
    hotkeyService.registerHotkey({ key: "i", ctrl: true }, () =>
      layoutStore.toggleInfo()
    );
    hotkeyService.registerHotkey({ key: "\\", ctrl: true }, () => {
      toggleNeuronPanel();
    });
  }

  // Esc 单独处理（保持原 drawer 关闭行为），不进服务
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      drawerSidebar = false;
      drawerInfo = false;
      drawerPanel = false;
    }
  }

  function closeDrawers() {
    drawerSidebar = false;
    drawerInfo = false;
    drawerPanel = false;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="app-layout" class:nav-open={activityOpen}>
  <nav class="activity-area">
    <!-- 顶栏布局图标已接管栏位开关，ActivityBar 仅保留底部连接入口 -->
    <ActivityBar
      items={[]}
      activeId={activityBarActive}
      onSelect={handleActivitySelect}
    >
      {#snippet footer()}
        <button
          class="activity-item"
          title={t("settings.title")}
          aria-label={t("settings.title")}
          onclick={() => (showSettingsDialog = true)}
        >
          <span class="activity-icon">
            <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
          </span>
        </button>
        <button
          class="activity-item"
          title={t("connectDialog.title")}
          aria-label={t("connectDialog.title")}
          onclick={() => (showConnectDialog = true)}
        >
          <span class="activity-icon">
            <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>
          </span>
        </button>
      {/snippet}
    </ActivityBar>
  </nav>

  <header class="status-area">
    <StatusBar
      appName={runtimeStatus?.app_name ?? "星脉"}
      sidebarVisible={layoutStore.state.sidebar.visible}
      infoVisible={layoutStore.state.info.visible}
      panelVisible={layoutStore.state.panel.visible}
      drawerSidebar={drawerSidebar}
      drawerInfo={drawerInfo}
      drawerPanel={drawerPanel}
      activityOpen={activityOpen}
      onToggleActivity={() => (activityOpen = !activityOpen)}
      onToggleSidebar={() => {
        if (window.innerWidth <= 800) drawerSidebar = !drawerSidebar;
        else layoutStore.toggleSidebar();
      }}
      onToggleInfo={() => {
        if (window.innerWidth <= 800) drawerInfo = !drawerInfo;
        else layoutStore.toggleInfo();
      }}
      onTogglePanel={() => {
        if (window.innerWidth <= 800) drawerPanel = !drawerPanel;
        else layoutStore.togglePanel();
      }}
    />
  </header>

  <!-- Desktop sidebar -->
  <div class="main-area">
    <aside class="sidebar-area desktop-only" style={sidebarStyle}>
      <ViewContainer containerId="sidebar" />
    </aside>

    <Splitter
      orientation="vertical"
      extraClass="desktop-only"
      onResize={(delta) => layoutStore.setSidebarWidth(layoutStore.state.sidebar.width + delta, false)}
      onResizeEnd={() => layoutStore.persistNow()}
    />

    <!-- Center column: editor + bottom panel -->
    <div class="center-column">
      <main class="chat-area" bind:this={mainRef}>
        <div class="chat-content">
          {#if mainPanes.length === 0}
            <div class="main-empty">
              <p>{t("common.mainEmpty")}</p>
            </div>
          {:else}
            <div class="main-panes">
              {#each mainPanes as pane, i (pane.id)}
                {#if i > 0}
                  <Splitter
                    orientation="vertical"
                    onResize={(delta) => handlePaneResize(i - 1, delta)}
                    onResizeEnd={() => layoutStore.persistNow()}
                  />
                {/if}
                <div
                  class="main-pane"
                  role="group"
                  class:active={pane.id === activePaneId}
                  style="flex-grow: {pane.grow};"
                  ondragover={handlePaneDragOver}
                  ondrop={(e) => handlePaneDrop(e, pane.id)}
                >
                  <!-- 每个分栏一个独立 tab 列表（分栏内面板可切换/拖拽重排/跨分栏移动） -->
                  <EditorTabs
                    tabs={paneTabs(pane)}
                    activeId={pane.activePanelId}
                    paneId={pane.id}
                    onSelect={(panelId) => layoutStore.setActivePanel(panelId)}
                    onClose={handleTabClose}
                    onDrop={(panelId, targetPaneId, targetIndex) =>
                      layoutStore.movePanel(panelId, targetPaneId, targetIndex)}
                    onDropToNewPane={(panelId) => layoutStore.movePanelToNewPane(panelId)}
                  />
                  <div class="pane-content">
                    {#each pane.panels as panel (panel.id)}
                      <div class="pane-view" class:visible={panel.id === pane.activePanelId}>
                        <!-- panel 传入 ViewHost：多实例面板（file-editor）经 context 获取自身实例 key -->
                        <ViewHost registration={viewForType(panel.type)} panel={panel} />
                      </div>
                    {/each}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </main>

      <!-- Bottom panel: only under the center main area -->
      <section class="panel-area desktop-only" style={panelStyle}>
        <Splitter
          orientation="horizontal"
          onResize={(delta) => layoutStore.setPanelHeight(layoutStore.state.panel.height - delta, false)}
          onResizeEnd={() => layoutStore.persistNow()}
        />
        <ViewContainer containerId="panel" />
      </section>
    </div>

    <Splitter
      orientation="vertical"
      extraClass="desktop-only"
      onResize={(delta) => layoutStore.setInfoWidth(layoutStore.state.info.width - delta, false)}
      onResizeEnd={() => layoutStore.persistNow()}
    />

    <!-- Desktop info panel -->
    <aside class="info-area desktop-only" style={infoStyle}>
      <ViewContainer containerId="info" />
    </aside>
  </div>

  <div class="error-area">
    <ErrorBanner
      message={error || dataStore.state.error}
      onDismiss={() => { error = ""; dataStore.state.error = ""; }}
    />
  </div>
</div>

<!-- Mobile drawer overlays -->
{#if drawerSidebar}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="drawer-backdrop" role="presentation" onclick={closeDrawers}></div>
  <aside class="drawer drawer-left">
    <div class="drawer-header">
      <h2>{t("drawer.sessions")}</h2>
      <button class="drawer-close" onclick={closeDrawers}>×</button>
    </div>
    <div class="drawer-body">
      <!-- 左抽屉对齐桌面 sidebar 容器：渲染容器内全部视图（sessions/topics/tools，tab 切换） -->
      <ViewContainer containerId="sidebar" />
    </div>
  </aside>
{/if}

{#if drawerInfo}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="drawer-backdrop" role="presentation" onclick={closeDrawers}></div>
  <aside class="drawer drawer-right">
    <div class="drawer-header">
      <h2>{t("drawer.info")}</h2>
      <button class="drawer-close" onclick={closeDrawers}>×</button>
    </div>
    <div class="drawer-body">
      <!-- 右抽屉对齐桌面 info 容器：渲染容器内全部视图（providers-models/neurons-list 等，tab 切换） -->
      <ViewContainer containerId="info" />
    </div>
  </aside>
{/if}

{#if drawerPanel}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="drawer-backdrop" role="presentation" onclick={closeDrawers}></div>
  <aside class="drawer drawer-bottom">
    <div class="drawer-header">
      <h2>{t("drawer.panel")}</h2>
      <button class="drawer-close" onclick={closeDrawers}>×</button>
    </div>
    <div class="drawer-body">
      <ViewContainer containerId="panel" />
    </div>
  </aside>
{/if}

<SessionCreateModal
  open={showCreateModal}
  onCreate={handleCreateSession}
  onClose={() => (showCreateModal = false)}
/>

<ConnectDialog
  open={showConnectDialog}
  locked={remoteConnLocked}
  onClose={() => {
    showConnectDialog = false;
    remoteConnLocked = false;
  }}
/>

<SettingsDialog
  open={showSettingsDialog}
  onClose={() => (showSettingsDialog = false)}
/>

<ConfirmDialog
  open={!!confirmReq}
  title={confirmReq?.title ?? ""}
  message={confirmReq?.message ?? ""}
  confirmLabel={confirmReq?.confirmLabel}
  danger={confirmReq?.danger}
  onConfirm={() => {
    confirmReq?.onConfirm();
    confirmReq = null;
  }}
  onCancel={() => (confirmReq = null)}
/>

<!-- 全局 git 写操作确认消费器（后端确认服务入队 → 跨面板常驻弹窗） -->
<GitConfirmHost />

{#if isTauriEnv}
  <!-- 无边框窗口边缘 resize 光标提示（Linux/WebKitGTK 下系统不渲染，见 spec window-edge-resize-cursor） -->
  <WindowEdgeResize />
{/if}

{#if !ready}
  <div class="loading-overlay">
    <p>Loading...</p>
  </div>
{/if}

<style>
  :global(body) {
    margin: 0;
    font-family: var(--font-body);
    font-size: var(--fs-base);
    line-height: 1.5;
    background: var(--color-bg);
    color: var(--color-text);
    overflow: hidden;
  }

  :global(*, *::before, *::after) {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
  }

  :global(h1, h2, h3, h4, h5, h6, p, ul, ol, pre) {
    margin: 0;
  }

  .app-layout {
    display: grid;
    height: 100%;
    grid-template-columns: auto 1fr;
    grid-template-rows: auto 1fr auto;
    grid-template-areas:
      "status status"
      "activity main"
      "error error";
    overflow: hidden;
  }

  .activity-area { grid-area: activity; display: flex; min-height: 0; }
  .status-area { grid-area: status; }

  .main-area {
    grid-area: main;
    display: flex;
    align-items: stretch;
    min-width: 0;
    min-height: 0;
  }

  /* 中间列：编辑区 + 底部面板（左右栏保持整高，不被底栏截断） */
  .center-column {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .sidebar-area {
    flex: none;
    width: 260px;
    overflow: hidden;
    background: var(--color-surface);
    border-right: var(--border-width) solid var(--color-border);
  }

  .chat-area {
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .chat-content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* main 区空态：默认无面板，提示从入口插入 */
  .main-empty {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    font-size: var(--fs-sm);
  }

  /* main 区分栏：flex 并排，分栏间由 Splitter 分隔；激活栏边框高亮 */
  .main-panes {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
  }
  .main-pane {
    flex: 1 1 0;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-right: var(--border-width) solid transparent;
  }
  .main-pane.active {
    border-right-color: var(--color-border);
  }
  /* 唯一分栏强制撑满：历史持久化可能残留非 1 的 grow（如 0.95），
     内联 style 的 flex-grow 优先级高于样式表，需 !important 兜底 */
  .main-panes > .main-pane:only-child {
    flex-grow: 1 !important;
  }
  /* 分栏内容区：tab 栏下方占据剩余高度 */
  .pane-content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  /* 分栏内面板：仅激活面板可见，其余保持挂载（隐藏）以保留状态 */
  .pane-view {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: none;
  }
  /* 列方向 flex：子元素（视图组件）在交叉轴（宽度）上 stretch 继承外层宽度 */
  .pane-view.visible {
    display: flex;
    flex-direction: column;
  }

  .info-area {
    flex: none;
    width: 280px;
    overflow: hidden;
    background: var(--color-surface);
    border-left: var(--border-width) solid var(--color-border);
  }

  .panel-area {
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    background: var(--color-surface);
    border-top: var(--border-width) solid var(--color-border);
  }

  .error-area { grid-area: error; }

  .loading-overlay {
    position: fixed; inset: 0;
    display: flex; align-items: center; justify-content: center;
    background: var(--color-bg); z-index: 200;
  }
  .loading-overlay p { font-size: var(--fs-lg); color: var(--color-text-muted); }

  /* ── Drawers ── */
  .drawer-backdrop {
    position: fixed; inset: 0; z-index: 50;
    background: rgba(0, 0, 0, 0.3);
  }

  .drawer {
    position: fixed; top: 0; bottom: 0; z-index: 60;
    width: 300px; max-width: 85vw;
    background: var(--color-surface);
    border-right: var(--border-width) solid var(--color-border);
    display: flex; flex-direction: column;
    animation: drawer-slidein var(--duration-normal) var(--ease-out);
  }

  .drawer-left { left: 0; }
  .drawer-right { right: 0; border-right: none; border-left: var(--border-width) solid var(--color-border); }

  /* 底部抽屉：覆盖 .drawer 的全高定位，改为从底部弹出的横向抽屉 */
  .drawer-bottom {
    top: auto;
    bottom: 0;
    left: 0;
    right: 0;
    width: 100%;
    max-width: 100%;
    height: 45vh;
    border-right: none;
    border-top: var(--border-width) solid var(--color-border);
    animation-name: drawer-slidein-bottom;
  }

  @keyframes drawer-slidein-bottom {
    from { transform: translateY(20px); opacity: 0; }
    to { transform: translateY(0); opacity: 1; }
  }

  @keyframes drawer-slidein {
    from { transform: translateX(-20px); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
  }

  .drawer-right {
    animation-name: drawer-slidein-right;
  }

  @keyframes drawer-slidein-right {
    from { transform: translateX(20px); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
  }

  .drawer-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: var(--space-3) var(--space-4);
    border-bottom: var(--border-width) solid var(--color-border);
  }

  .drawer-header h2 {
    margin: 0; font-size: var(--fs-base); font-weight: 600;
  }

  .drawer-close {
    background: none; border: none; font-size: 22px; cursor: pointer;
    color: var(--color-text); padding: 0 4px; line-height: 1;
  }

  .drawer :global(.sidebar) {
    width: 100% !important;
    border-right: none;
  }

  .drawer-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .drawer-body :global(> *) { flex: 1; min-height: 0; }

  /* ── Responsive: <800px hide desktop panels, show drawers ── */
  @media (max-width: 800px) {
    .desktop-only { display: none; }

    /* 小屏默认隐藏左侧导航栏，点击顶栏 logo 切换显隐（nav-open 显示） */
    .activity-area { display: none; }
    .app-layout.nav-open .activity-area { display: flex; }

    .app-layout {
      grid-template-rows: auto 1fr auto;
      grid-template-areas:
        "status status"
        "activity main"
        "error error";
    }

    .main-area { min-width: 0; }
  }

  @media (min-width: 801px) {
    /* On desktop, only show sidebar inline when not in drawer mode */
    .drawer-backdrop,
    .drawer { display: none; }
  }
</style>

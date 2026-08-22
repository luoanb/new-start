// Runes（$state/$derived）在 .svelte.ts 中由 Svelte 编译器直接转换，无需 import。
import type {
  LayoutState,
  MainPanel,
  MainPane,
  MainPanelType,
  ViewContainerId,
} from "./layoutTypes";
import {
  DEFAULT_LAYOUT,
  BOUNDS,
  clamp,
  normalizePaneTarget,
} from "./layoutTypes";
import { LocalStorageLayoutStorage, type LayoutStorage } from "./layoutStorage";

const storage: LayoutStorage = new LocalStorageLayoutStorage();

const state = $state<LayoutState>(storage.load() ?? DEFAULT_LAYOUT);

/** 待定位锚点：面板「在会话中定位」请求 → ChatArea 消费后清除。 */
export type LocateAnchor = {
  conversationId: string;
  messageIndex: number;
};

let locateAnchor = $state<LocateAnchor | null>(null);

function persist() {
  storage.save(state);
}

/** 从指定容器移除视图；若移除的是激活视图，回退到容器首个视图（无则置空）。 */
function removeViewFromContainer(containerId: ViewContainerId, viewId: string): void {
  const c = state.containers[containerId];
  if (!c) return;
  const idx = c.views.indexOf(viewId);
  if (idx >= 0) {
    c.views.splice(idx, 1);
    if (c.activeView === viewId) c.activeView = c.views[0] ?? "";
  }
}

/**
 * 分栏数变化后收敛：唯一分栏的 grow 权重无意义。
 * 历史拖拽可能残留非 1 的 grow（如 0.95/0.2），关闭一个分栏后剩余分栏若保留 0.95，
 * WebKitGTK 下 flex-grow < 1 会导致唯一分栏宽度不满，故归一化为 1。
 */
function normalizeSinglePaneGrow(): void {
  if (state.main.panes.length === 1) {
    state.main.panes[0].grow = 1;
  }
}

/** 生成面板/分栏唯一 ID。
 * crypto.randomUUID 仅在安全上下文（HTTPS/localhost）可用；
 * pad/远程浏览器经 HTTP（非安全上下文）访问时缺失，回退时间戳+随机数保证本地唯一。 */
function uuid(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `id-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

/** 从所有视图容器移除 viewId（移动/隐藏共用的前置步骤）。 */
function detachView(viewId: string): void {
  for (const cid of Object.keys(state.containers) as ViewContainerId[]) {
    removeViewFromContainer(cid, viewId);
  }
  const hi = state.hiddenViews.indexOf(viewId);
  if (hi >= 0) state.hiddenViews.splice(hi, 1);
}

export const layoutStore = {
  state,
  /** 待定位锚点（响应式）：非空时 ChatArea 应滚动高亮该会话的锚点消息。 */
  locateAnchor,

  /** 请求定位：跳到指定会话并滚动高亮锚点消息（ChatArea 消费后自动清除）。 */
  requestLocate(conversationId: string, messageIndex: number) {
    locateAnchor = { conversationId, messageIndex };
  },
  clearLocate() {
    locateAnchor = null;
  },

  /** 拖动结束后手动持久化（拖动中 setXxx(persistNow=false) 避免高频写入） */
  persistNow: persist,

  toggleSidebar() {
    state.sidebar.visible = !state.sidebar.visible;
    persist();
  },
  toggleInfo() {
    state.info.visible = !state.info.visible;
    persist();
  },
  togglePanel() {
    state.panel.visible = !state.panel.visible;
    persist();
  },

  setSidebarWidth(width: number, persistNow = true) {
    state.sidebar.width = clamp(width, BOUNDS.sidebar.min, BOUNDS.sidebar.max);
    if (persistNow) persist();
  },
  setInfoWidth(width: number, persistNow = true) {
    state.info.width = clamp(width, BOUNDS.info.min, BOUNDS.info.max);
    if (persistNow) persist();
  },
  setPanelHeight(height: number, persistNow = true) {
    state.panel.height = clamp(height, BOUNDS.panel.min, window.innerHeight * 0.6);
    if (persistNow) persist();
  },

  /** 切换指定视图容器内的激活视图（tab 点击）。 */
  setContainerView(containerId: ViewContainerId, viewId: string) {
    const c = state.containers[containerId];
    if (!c || !c.views.includes(viewId)) return;
    c.activeView = viewId;
    persist();
  },

  /** 跨容器移动视图（拖拽）：从所有容器脱离后挂入目标容器并激活。 */
  moveView(viewId: string, targetContainerId: ViewContainerId) {
    detachView(viewId);
    const target = state.containers[targetContainerId];
    if (!target.views.includes(viewId)) target.views.push(viewId);
    target.activeView = viewId;
    persist();
  },

  /** 同一容器内重排视图（tab 拖拽落点插入）：先移除后按目标索引插入。 */
  reorderView(containerId: ViewContainerId, viewId: string, targetIndex: number) {
    const c = state.containers[containerId];
    if (!c) return;
    const idx = c.views.indexOf(viewId);
    if (idx < 0) return;
    const [view] = c.views.splice(idx, 1);
    const insertAt = Math.max(
      0,
      Math.min(targetIndex > idx ? targetIndex - 1 : targetIndex, c.views.length),
    );
    c.views.splice(insertAt, 0, view);
    persist();
  },

  /** 隐藏视图（移入 hiddenViews，可从 ⋯ 菜单重新显示）。 */
  hideView(viewId: string) {
    detachView(viewId);
    if (!state.hiddenViews.includes(viewId)) state.hiddenViews.push(viewId);
    persist();
  },

  /** 显示隐藏视图：挂入目标容器并激活。 */
  showView(viewId: string, targetContainerId: ViewContainerId) {
    detachView(viewId);
    const target = state.containers[targetContainerId];
    if (!target.views.includes(viewId)) target.views.push(viewId);
    target.activeView = viewId;
    persist();
  },

  /**
   * 插入/激活一个 main 区面板。
   * - 默认同一类型全局唯一：已存在（任意分栏）则仅激活其所在分栏与该面板。
   * - 多实例类型（如 file-editor）：传入 `instanceId` 时按实例 id 区分——
   *   已存在同 instanceId 的面板则激活；否则新建面板并以其为 id。
   * - target：目标分栏索引（0 基）；"new" 或 >= 当前栏数 → 新增一栏；默认 0；非法值收敛。
   * - 插入到既有栏时追加到该栏 panels[] 并激活（同一分栏可 tab 切换多个面板）。
   * @returns 面板实例 id（供 closePanel 关闭）。
   */
  insertPanel(type: MainPanelType, target?: number | "new", instanceId?: string): string {
    const existing = state.main.panes
      .flatMap((p) => p.panels.map((x) => ({ pane: p, panel: x })))
      .find((x) => (instanceId ? x.panel.id === instanceId : x.panel.type === type));
    if (existing) {
      state.main.activePaneId = existing.pane.id;
      existing.pane.activePanelId = existing.panel.id;
      persist();
      return existing.panel.id;
    }
    const idx = normalizePaneTarget(target, state.main.panes.length);
    const panel: MainPanel = { id: instanceId ?? uuid(), type };
    let paneId: string;
    if (idx >= state.main.panes.length) {
      // 新增一栏（首个面板）
      const pane: MainPane = {
        id: uuid(),
        grow: 1,
        panels: [panel],
        activePanelId: panel.id,
      };
      state.main.panes.push(pane);
      paneId = pane.id;
    } else {
      // 追加到既有分栏并激活
      const pane = state.main.panes[idx];
      pane.panels.push(panel);
      pane.activePanelId = panel.id;
      paneId = pane.id;
    }
    state.main.activePaneId = paneId;
    persist();
    return panel.id;
  },

  /** 关闭指定面板：移除面板；所在分栏仍有余面板则激活相邻面板，否则移除空分栏（收缩）。 */
  closePanel(panelId: string) {
    const idx = state.main.panes.findIndex((p) => p.panels.some((x) => x.id === panelId));
    if (idx < 0) return;
    const pane = state.main.panes[idx];
    const panelIdx = pane.panels.findIndex((x) => x.id === panelId);
    pane.panels.splice(panelIdx, 1);
    if (pane.panels.length > 0) {
      if (pane.activePanelId === panelId) {
        pane.activePanelId = pane.panels[Math.min(panelIdx, pane.panels.length - 1)]?.id ?? null;
      }
    } else {
      // 分栏已空 → 移除分栏并激活相邻分栏
      const paneId = pane.id;
      state.main.panes.splice(idx, 1);
      normalizeSinglePaneGrow();
      if (state.main.activePaneId === paneId) {
        const fallback =
          state.main.panes[Math.min(idx, state.main.panes.length - 1)] ??
          state.main.panes[0];
        state.main.activePaneId = fallback?.id ?? null;
      }
    }
    persist();
  },

  /** 激活指定面板：同时激活其所在分栏（分栏内 tab 点击）。 */
  setActivePanel(panelId: string) {
    const pane = state.main.panes.find((p) => p.panels.some((x) => x.id === panelId));
    if (!pane) return;
    pane.activePanelId = panelId;
    state.main.activePaneId = pane.id;
    persist();
  },

  /**
   * 拖动移动面板（tab 拖拽）。
   * - 同一分栏内 = 重排（targetIndex 为插入位置，自动处理移除后的索引回退）。
   * - 跨分栏 = 迁移到目标分栏并激活；源分栏被移空后自动移除（收缩）。
   */
  movePanel(panelId: string, targetPaneId: string, targetIndex: number) {
    const srcPane = state.main.panes.find((p) => p.panels.some((x) => x.id === panelId));
    if (!srcPane) return;
    const srcIdx = srcPane.panels.findIndex((x) => x.id === panelId);
    const [panel] = srcPane.panels.splice(srcIdx, 1);

    if (srcPane.id === targetPaneId) {
      // 同分栏重排：先移除后插入，目标索引需回退一格
      const insertAt = Math.max(
        0,
        Math.min(
          targetIndex > srcIdx ? targetIndex - 1 : targetIndex,
          srcPane.panels.length,
        ),
      );
      srcPane.panels.splice(insertAt, 0, panel);
    } else {
      const dstPane = state.main.panes.find((p) => p.id === targetPaneId);
      if (!dstPane) {
        // 目标分栏不存在（异常兜底）：退回源分栏原位
        srcPane.panels.splice(Math.min(srcIdx, srcPane.panels.length), 0, panel);
        return;
      }
      const insertAt = Math.max(0, Math.min(targetIndex, dstPane.panels.length));
      dstPane.panels.splice(insertAt, 0, panel);
      dstPane.activePanelId = panel.id;
      state.main.activePaneId = dstPane.id;
      if (srcPane.panels.length === 0) {
        // 源分栏已空 → 移除收缩
        state.main.panes.splice(state.main.panes.indexOf(srcPane), 1);
        normalizeSinglePaneGrow();
      } else if (srcPane.activePanelId === panelId) {
        // 被移动的正是源分栏激活面板 → 激活相邻面板
        srcPane.activePanelId =
          srcPane.panels[Math.min(srcIdx, srcPane.panels.length - 1)]?.id ?? null;
      }
    }
    persist();
  },

  /** 调整分栏宽度权重（拖拽中 persistNow=false 避免高频写入）。 */
  setPaneGrow(paneId: string, grow: number, persistNow = true) {
    const pane = state.main.panes.find((p) => p.id === paneId);
    if (!pane) return;
    pane.grow = clamp(grow, BOUNDS.paneGrow.min, BOUNDS.paneGrow.max);
    if (persistNow) persist();
  },

  /**
   * 拖动到「新建分栏」目标：把面板移入一个新分栏（追加到最右并激活）；
   * 源分栏被移空后自动移除（收缩），否则激活相邻面板。
   */
  movePanelToNewPane(panelId: string) {
    const srcPane = state.main.panes.find((p) => p.panels.some((x) => x.id === panelId));
    if (!srcPane) return;
    const srcIdx = srcPane.panels.findIndex((x) => x.id === panelId);
    const [panel] = srcPane.panels.splice(srcIdx, 1);
    const pane: MainPane = {
      id: uuid(),
      grow: 1,
      panels: [panel],
      activePanelId: panel.id,
    };
    state.main.panes.push(pane);
    state.main.activePaneId = pane.id;
    if (srcPane.panels.length === 0) {
      state.main.panes.splice(state.main.panes.indexOf(srcPane), 1);
      normalizeSinglePaneGrow();
    } else if (srcPane.activePanelId === panelId) {
      srcPane.activePanelId =
        srcPane.panels[Math.min(srcIdx, srcPane.panels.length - 1)]?.id ?? null;
    }
    persist();
  },

  setActivity(id: string | null) {
    state.activity.active = id;
    persist();
  },
};

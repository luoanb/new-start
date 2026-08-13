/**
 * ViewContext —— 容器与内容解耦的核心。
 *
 * 参考 VS Code 的 view provider 模型：容器只消费注册表（id → component），
 * 视图组件通过 context 自取所需（数据 store / 命令 / 会话级 UI 状态），
 * 而非由组合根逐个传 props。新增视图 = 注册表加一条，零改 +page.svelte。
 */
import { getContext, setContext } from "svelte";
import { dataStore } from "$lib/stores/dataStore.svelte";
import { layoutStore } from "./LayoutStore.svelte";

export const VIEW_CTX_KEY = "openclaw:view-ctx";

/** 组合根（+page.svelte）提供的视图命令集合。 */
export type ViewCommands = {
  sendMessage: (text: string) => Promise<void>;
  selectConversation: (id: string) => void;
  createSession: (mode: string) => Promise<void>;
  closeSession: (id: string) => Promise<void>;
  changeModel: (providerId: string, modelId: string) => void;
  openCreateModal: () => void;
  showError: (msg: string) => void;
  dismissError: () => void;
  /** 在 main 区打开工具配置编辑面板（与对话同级的独立面板）。 */
  openToolEditor: () => void;
  /** 关闭工具配置编辑面板，回到打开前的 main 区视图。 */
  closeToolEditor: () => void;
  /** 关闭服务商/模型管理编辑面板，回到打开前的 main 区视图。 */
  closeProviderManager: () => void;
};

/** 会话级 UI 状态（组合根持有，$state 保证响应式传播）。
 * 运行状态以 dataStore.runningSessions 为唯一权威来源（后端多会话并行）；
 * sendingIds 仅为发送按钮防抖锁，拦截同一会话连点重复发送，不参与运行状态判定。 */
export type ViewUiState = {
  activeProviderId: string;
  activeModelId: string;
  sendingIds: Set<string>;
};

export type ViewContext = {
  stores: {
    data: typeof dataStore;
    layout: typeof layoutStore;
  };
  ui: ViewUiState;
  commands: ViewCommands;
};

export function setViewContext(ctx: ViewContext): void {
  setContext(VIEW_CTX_KEY, ctx);
}

export function useViewContext(): ViewContext {
  return getContext<ViewContext>(VIEW_CTX_KEY);
}

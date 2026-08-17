/**
 * 视图注册表 —— ActivityBar / EditorTabs / 视图容器（ViewContainer）统一从这里消费。
 *
 * 容器与内容解耦的边界：容器只认识 ViewRegistration（id/title/icon/component），
 * 组件实例的挂载由 ViewHost 完成；各视图需要的数据/命令通过 ViewContext 自取。
 * 新增视图 = 在此注册一条记录，+page.svelte 无需改动。
 */
import type { Component } from "svelte";
import type { ViewContainerId } from "./layoutTypes";
import SessionList from "$lib/components/SessionList.svelte";
import ProvidersModelsPanel from "$lib/components/ProvidersModelsPanel.svelte";
import ProviderManager from "$lib/components/ProviderManager.svelte";
import TopicPanel from "$lib/components/TopicPanel.svelte";
import PollerPanel from "$lib/components/PollerPanel.svelte";
import ToolPanel from "$lib/components/ToolPanel.svelte";
import LogPanel from "$lib/components/LogPanel.svelte";
import ChatArea from "$lib/components/ChatArea.svelte";
import NeuronManager from "$lib/components/NeuronManager.svelte";
import NeuronListPanel from "$lib/components/NeuronListPanel.svelte";
import ToolEditor from "$lib/components/ToolEditor.svelte";
import FileEditor from "$lib/components/FileEditor.svelte";
import FileExplorer from "$lib/components/FileExplorer.svelte";
import type { MainPanelType } from "./layoutTypes";

export type { ViewContainerId } from "./layoutTypes";

export type ViewMeta = {
  id: string;
  label: string;
  icon?: string;
  /** 动态标题（原始文本，非 i18n key）：提供时优先展示 */
  title?: string;
  /** 动态标题截断展示（限制宽度，如对话标题） */
  truncate?: boolean;
  /** 未保存标记（●）：文件编辑器 tab 未保存时显示 */
  dirty?: boolean;
  /** 完整悬停提示（如文件 tab 的完整路径） */
  tooltip?: string;
  /** icon 色调（对齐会话列表 mode-badge 色板）：chat/agent/assistant/system */
  iconTone?: string;
};

export type ViewRegistration = {
  id: string;
  title: string;
  icon?: string;
  component: Component;
  /** 允许被拖拽到的容器；"*" = 任意视图容器。缺省 = 不可移动（仅主区内部视图）。 */
  movableTo?: ViewContainerId[] | "*";
};

/** 视图容器注册表（sidebar / info / panel 共享）。title/label 存 i18n key（views.*），渲染处以 t() 解析。 */
export const viewRegistry: Record<string, ViewRegistration> = {
  sessions: {
    id: "sessions",
    title: "views.sessions",
    icon: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/></svg>',
    component: SessionList,
    movableTo: "*",
  },
  // v10: 服务商+模型聚合为单视图（服务商分组，模型为子项；支持管理入口）
  "providers-models": {
    id: "providers-models",
    title: "views.providersModels",
    icon: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="7" rx="2"/><rect x="2" y="14" width="20" height="7" rx="2"/><line x1="6" y1="6.5" x2="6.01" y2="6.5"/><line x1="6" y1="17.5" x2="6.01" y2="17.5"/></svg>',
    component: ProvidersModelsPanel,
    movableTo: "*",
  },
  // v9: 神经元统一管理列表（info 容器，《模型》之后）
  "neurons-list": {
    id: "neurons-list",
    title: "views.neuronsList",
    icon: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="5" cy="6" r="2"/><circle cx="19" cy="7" r="2"/><circle cx="12" cy="18" r="2"/><line x1="6.5" y1="7" x2="11" y2="16"/><line x1="17.5" y1="8" x2="13" y2="16"/></svg>',
    component: NeuronListPanel,
    movableTo: "*",
  },
  topics: {
    id: "topics",
    title: "views.topics",
    icon: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z"/><line x1="7" y1="7" x2="7.01" y2="7"/></svg>',
    component: TopicPanel,
    movableTo: "*",
  },
  poller: {
    id: "poller",
    title: "views.poller",
    icon: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg>',
    component: PollerPanel,
    movableTo: "*",
  },
  tools: {
    id: "tools",
    title: "views.tools",
    icon: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/></svg>',
    component: ToolPanel,
    movableTo: "*",
  },
  logs: {
    id: "logs",
    title: "views.logs",
    icon: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/></svg>',
    component: LogPanel,
    movableTo: "*",
  },
  // 文件管理：文件树（sidebar 默认挂载，VSCode 资源管理器语义）
  files: {
    id: "files",
    title: "views.files",
    icon: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>',
    component: FileExplorer,
    movableTo: "*",
  },
};

/** main 区域（editor area）专用视图：走 EditorTabs + split 语义，不进入视图容器。 */
export const mainViews: ViewRegistration[] = [
  { id: "chat", title: "views.chat", component: ChatArea },
  { id: "neurons", title: "views.neurons", component: NeuronManager },
  { id: "tool-editor", title: "views.toolEditor", component: ToolEditor },
  { id: "provider-manager", title: "views.providerManager", component: ProviderManager },
  { id: "file-editor", title: "views.fileEditor", component: FileEditor },
];

/** Activity Bar 入口（icon 轨）。chat 用于向 main 区插入会话面板。 */
export const activityItems: ViewMeta[] = [
  {
    id: "sessions",
    label: "views.sessions",
    icon: '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/></svg>',
  },
  {
    id: "chat",
    label: "views.chat",
    icon: '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>',
  },
];

/**
 * main 区面板元数据（tab 栏 / 空态提示共用）。
 * icon 统一 16x16 SVG，与侧栏视图图标风格对齐。
 */
export const mainPanelMeta: Record<MainPanelType, { label: string; icon: string }> = {
  chat: {
    label: "views.chat",
    icon: '<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>',
  },
  neurons: {
    label: "views.neurons",
    icon: '<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="5" cy="6" r="2"/><circle cx="19" cy="7" r="2"/><circle cx="12" cy="18" r="2"/><line x1="6.5" y1="7" x2="11" y2="16"/><line x1="17.5" y1="8" x2="13" y2="16"/></svg>',
  },
  "tool-editor": {
    label: "views.toolEditor",
    icon: '<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 3a2.83 2.83 0 0 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/><path d="m15 5 4 4"/></svg>',
  },
  "provider-manager": {
    label: "views.providerManager",
    icon: '<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="7" rx="2"/><rect x="2" y="14" width="20" height="7" rx="2"/><line x1="6" y1="6.5" x2="6.01" y2="6.5"/><line x1="6" y1="17.5" x2="6.01" y2="17.5"/></svg>',
  },
  "file-editor": {
    label: "views.fileEditor",
    icon: '<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>',
  },
};

export function getRegistration(id: string): ViewRegistration | undefined {
  return viewRegistry[id];
}

/** 视图是否可被移动到指定容器。 */
export function canMoveTo(viewId: string, containerId: ViewContainerId): boolean {
  const reg = viewRegistry[viewId];
  if (!reg?.movableTo) return false;
  return reg.movableTo === "*" || reg.movableTo.includes(containerId);
}

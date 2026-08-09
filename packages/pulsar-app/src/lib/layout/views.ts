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
import ProvidersPanel from "$lib/components/ProvidersPanel.svelte";
import ModelsPanel from "$lib/components/ModelsPanel.svelte";
import TopicPanel from "$lib/components/TopicPanel.svelte";
import PollerPanel from "$lib/components/PollerPanel.svelte";
import ToolPanel from "$lib/components/ToolPanel.svelte";
import LogPanel from "$lib/components/LogPanel.svelte";
import ChatArea from "$lib/components/ChatArea.svelte";
import NeuronManager from "$lib/components/NeuronManager.svelte";
import ToolEditor from "$lib/components/ToolEditor.svelte";
import SessionSpecsPanel from "$lib/components/SessionSpecsPanel.svelte";
import type { MainPanelType } from "./layoutTypes";

export type { ViewContainerId } from "./layoutTypes";

export type ViewMeta = {
  id: string;
  label: string;
  icon?: string;
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
  sessions: { id: "sessions", title: "views.sessions", component: SessionList, movableTo: "*" },
  // 原 Info 组合面板拆分为三个独立视图（技能并入 Tools，不再单独展示）
  providers: { id: "providers", title: "views.providers", component: ProvidersPanel, movableTo: "*" },
  models: { id: "models", title: "views.models", component: ModelsPanel, movableTo: "*" },
  topics: { id: "topics", title: "views.topics", component: TopicPanel, movableTo: "*" },
  poller: { id: "poller", title: "views.poller", component: PollerPanel, movableTo: "*" },
  tools: { id: "tools", title: "views.tools", component: ToolPanel, movableTo: "*" },
  logs: { id: "logs", title: "views.logs", component: LogPanel, movableTo: "*" },
};

/** main 区域（editor area）专用视图：走 EditorTabs + split 语义，不进入视图容器。 */
export const mainViews: ViewRegistration[] = [
  { id: "chat", title: "views.chat", component: ChatArea },
  { id: "neurons", title: "views.neurons", component: NeuronManager },
  { id: "tool-editor", title: "views.toolEditor", component: ToolEditor },
  { id: "session-specs", title: "views.sessionSpecs", component: SessionSpecsPanel },
];

/** Activity Bar 入口（icon 轨）。chat 用于向 main 区插入会话面板。 */
export const activityItems: ViewMeta[] = [
  {
    id: "sessions",
    label: "views.sessions",
    icon: '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M21 11.5a8.38 8.38 0 0 1-8.5 8.5 8.5 8.5 0 0 1-3.8-.9L3 21l1.9-5.7a8.5 8.5 0 0 1-.9-3.8A8.38 8.38 0 0 1 12.5 3 8.38 8.38 0 0 1 21 11.5z"/></svg>',
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
  "session-specs": {
    label: "views.sessionSpecs",
    icon: '<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M8 8h8M8 12h8M8 16h5"/></svg>',
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

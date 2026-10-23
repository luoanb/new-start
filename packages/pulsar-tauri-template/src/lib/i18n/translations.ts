/**
 * i18n 词典：本包只保留「基建 shell」所需的文案段。
 *
 * 段落划分（业务包按需新增段，勿复用本文件里不存在的 key：
 * `t()` 查不到时回落为 key 原文）：
 * - locale / common / settings / themeSwitcher：外壳通用
 * - editorTabs：主区 tab 栏
 * - chatMessage：复制类通用文案（CopyButton 复用）
 * - logPanel：日志面板
 * - views：视图标题（与 `layout/views.ts` 注册表的 `title/label` 一一对应）
 */
export type Translations = {
  locale: { label: string };
  common: {
    newPane: string;
    confirm: string;
    cancel: string;
    close: string;
    clickToCopy: string;
    copied: string;
    copyFailed: string;
    toggleSidebar: string;
    toggleInfo: string;
    togglePanel: string;
    newExamplePanel: string;
    minimize: string;
    maximize: string;
    restore: string;
    closeWindow: string;
  };
  editorTabs: {
    close: string;
    closeOthers: string;
    closeRight: string;
    closeAll: string;
    closeSelected: string;
    copyPath: string;
    moveNewPane: string;
    selectSameType: string;
    selectAll: string;
    deselect: string;
    pinned: string;
  };
  chatMessage: {
    copy: string;
    copied: string;
  };
  themeSwitcher: {
    light: string;
    dark: string;
    system: string;
  };
  settings: {
    title: string;
    theme: string;
    language: string;
  };
  logPanel: {
    verbosity: string;
    minLevel: string;
    phase: string;
    phaseSearch: string;
    target: string;
    targetPlaceholder: string;
    keyword: string;
    keywordPlaceholder: string;
    clear: string;
    file: string;
    empty: string;
    detail: string;
    copy: string;
    copied: string;
    copyFailed: string;
    close: string;
    initFailed: string;
    setLevelFailed: string;
    clearFailed: string;
  };
  views: {
    logs: string;
    example: string;
    exampleHint: string;
    exampleReady: string;
    lastEvent: string;
    exampleOpenPanel: string;
    exampleTriggerError: string;
    exampleError: string;
  };
};

export const en: Translations = {
  locale: { label: "English" },
  common: {
    newPane: "New Pane",
    confirm: "Confirm",
    cancel: "Cancel",
    close: "Close",
    clickToCopy: "Click to copy",
    copied: "Copied",
    copyFailed: "Copy failed",
    toggleSidebar: "Toggle sidebar",
    toggleInfo: "Toggle info panel",
    togglePanel: "Toggle bottom panel",
    newExamplePanel: "Open example panel",
    minimize: "Minimize",
    maximize: "Maximize",
    restore: "Restore",
    closeWindow: "Close",
  },
  editorTabs: {
    close: "Close",
    closeOthers: "Close Others",
    closeRight: "Close to the Right",
    closeAll: "Close All",
    closeSelected: "Close Selected ({count})",
    copyPath: "Copy Path",
    moveNewPane: "Move to New Pane",
    selectSameType: "Select Same Type",
    selectAll: "Select All",
    deselect: "Deselect",
    pinned: "Pinned",
  },
  chatMessage: {
    copy: "Copy",
    copied: "Copied",
  },
  themeSwitcher: {
    light: "Light",
    dark: "Dark",
    system: "System",
  },
  settings: {
    title: "Settings",
    theme: "Theme",
    language: "Language",
  },
  logPanel: {
    verbosity: "Verbosity",
    minLevel: "Min level",
    phase: "Phase",
    phaseSearch: "type to filter phase…",
    target: "Target",
    targetPlaceholder: "module / target…",
    keyword: "Keyword",
    keywordPlaceholder: "phase / error_code…",
    clear: "Clear",
    file: "file",
    empty: "No log entries match the current filters.",
    detail: "Detail",
    copy: "Copy",
    copied: "Copied",
    copyFailed: "Copy failed",
    close: "Close",
    initFailed: "Logs init failed: {error}",
    setLevelFailed: "Set level failed: {error}",
    clearFailed: "Clear failed: {error}",
  },
  views: {
    logs: "Logs",
    example: "Example",
    exampleHint: "This view is registered in layout/views.ts and pulls everything from ViewContext.",
    exampleReady: "Backend ready",
    lastEvent: "Last event",
    exampleOpenPanel: "Open as main panel",
    exampleTriggerError: "Trigger error banner",
    exampleError: "Example error: raised by the example view.",
  },
};

export const zh: Translations = {
  locale: { label: "中文" },
  common: {
    newPane: "新建分栏",
    confirm: "确认",
    cancel: "取消",
    close: "关闭",
    clickToCopy: "点击复制",
    copied: "已复制",
    copyFailed: "复制失败",
    toggleSidebar: "折叠/展开侧栏",
    toggleInfo: "折叠/展开信息栏",
    togglePanel: "折叠/展开底部面板",
    newExamplePanel: "打开示例面板",
    minimize: "最小化",
    maximize: "最大化",
    restore: "还原",
    closeWindow: "关闭窗口",
  },
  editorTabs: {
    close: "关闭",
    closeOthers: "关闭其他",
    closeRight: "关闭右侧",
    closeAll: "全部关闭",
    closeSelected: "关闭选中 ({count})",
    copyPath: "复制路径",
    moveNewPane: "移动到新分栏",
    selectSameType: "选择同类",
    selectAll: "全选",
    deselect: "取消选择",
    pinned: "已固定",
  },
  chatMessage: {
    copy: "复制",
    copied: "已复制",
  },
  themeSwitcher: {
    light: "浅色",
    dark: "深色",
    system: "跟随系统",
  },
  settings: {
    title: "设置",
    theme: "主题",
    language: "语言",
  },
  logPanel: {
    verbosity: "日志级别",
    minLevel: "最低级别",
    phase: "阶段",
    phaseSearch: "输入以筛选阶段…",
    target: "来源",
    targetPlaceholder: "模块 / 目标…",
    keyword: "关键字",
    keywordPlaceholder: "阶段 / 错误码…",
    clear: "清空",
    file: "文件",
    empty: "没有匹配当前筛选条件的日志。",
    detail: "详情",
    copy: "复制",
    copied: "已复制",
    copyFailed: "复制失败",
    close: "关闭",
    initFailed: "日志初始化失败：{error}",
    setLevelFailed: "设置级别失败：{error}",
    clearFailed: "清空失败：{error}",
  },
  views: {
    logs: "日志",
    example: "示例视图",
    exampleHint: "本视图注册于 layout/views.ts，数据与命令全部经 ViewContext 自取。",
    exampleReady: "后端就绪",
    lastEvent: "最近事件",
    exampleOpenPanel: "在主区打开",
    exampleTriggerError: "触发错误横幅",
    exampleError: "示例错误：由示例视图抛出。",
  },
};

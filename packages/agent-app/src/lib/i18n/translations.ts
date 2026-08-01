export type Translations = {
  locale: { label: string };
  common: {
    send: string;
    sending: string;
    thinking: string;
    noModel: string;
    session: string;
  };
  statusBar: {
    appName: string;
    toggleSidebar: string;
    toggleInfo: string;
    togglePanel: string;
  };
  sessionList: {
    title: string;
    empty: string;
    create: string;
    msgs: string;
  };
  chatArea: {
    emptyTitle: string;
    emptyDesc: string;
    chatInputPlaceholder: string;
  };
  chatMessage: {
    you: string;
    assistant: string;
    system: string;
  };
  sidePanel: {
    providers: string;
    models: string;
    skills: string;
    noProviders: string;
    noModels: string;
    noSkills: string;
    id: string;
    auth: string;
    api: string;
    kind: string;
    context: string;
    output: string;
    tokens: string;
    mIn: string;
    mOut: string;
    caps: Record<string, string>;
  };
  createModal: {
    title: string;
    hint: string;
    chatLabel: string;
    chatDesc: string;
    agentLabel: string;
    agentDesc: string;
    assistantLabel: string;
    assistantDesc: string;
  };
  themeSwitcher: {
    light: string;
    dark: string;
    system: string;
  };
  toolCall: {
    arguments: string;
  };
  drawer: {
    sessions: string;
    info: string;
  };
  topicPanel: {
    topics: string;
    all: string;
    create: string;
    createTitle: string;
    createName: string;
    createDesc: string;
    createSubmit: string;
    creating: string;
    noTopics: string;
    deleteConfirm: string;
    cancel: string;
    confirm: string;
    pause: string;
    resume: string;
    progress: string;
    scopeItems: string;
    addScopeItem: string;
    scopeGoal: string;
    scopeContract: string;
    scopeAdd: string;
    scopeStatusPending: string;
    scopeStatusDone: string;
    status: string;
    description: string;
    updated: string;
    sessionId: string;
    name: string;
  };
  pollerPanel: {
    poller: string;
    status: string;
    running: string;
    paused: string;
    tickCount: string;
    taskCount: string;
    interval: string;
    pause: string;
    resume: string;
    trigger: string;
    triggering: string;
    noPoller: string;
    pendingTrigger: string;
  };
  neuronPanel: {
    neurons: string;
    list: string;
    detail: string;
    network: string;
    noNeurons: string;
    weight: string;
    systemType: string;
    description: string;
    content: string;
    toolIds: string;
    createdAt: string;
    updatedAt: string;
    edit: string;
    save: string;
    saving: string;
    cancel: string;
    connections: string;
    source: string;
    target: string;
    connectionWeight: string;
    networkTitle: string;
    networkDepth: string;
    viewModeGraph: string;
    viewModeTree: string;
    edgeWeight: string;
    depthLabel: string;
    edgeTypeLabel: string;
    edgeFloating: string;
    edgeBezier: string;
    edgeSmoothstep: string;
    edgeStep: string;
    edgeStraight: string;
    back: string;
    viewNetwork: string;
    jumpTo: string;
    loading: string;
    search: string;
    filterAll: string;
    connectionsCount: string;
    emptyTitle: string;
    emptyHint: string;
    startAssistant: string;
    focusNode: string;
    drawerTitle: string;
    close: string;
  };
};

export const en: Translations = {
  locale: { label: "English" },
  common: {
    send: "Send",
    sending: "Sending...",
    thinking: "Thinking...",
    noModel: "No model selected",
    session: "Session",
  },
  statusBar: {
    appName: "Agent App",
    toggleSidebar: "Toggle sidebar",
    toggleInfo: "Toggle info panel",
    togglePanel: "Toggle bottom panel",
  },
  sessionList: {
    title: "Sessions",
    empty: "No sessions yet.",
    create: "Create one",
    msgs: "msgs",
  },
  chatArea: {
    emptyTitle: "Start a conversation",
    emptyDesc: "Send a message below to begin",
    chatInputPlaceholder: "Type a message... (Enter to send, Shift+Enter for new line)",
  },
  chatMessage: {
    you: "You",
    assistant: "Assistant",
    system: "system",
  },
  sidePanel: {
    providers: "Providers",
    models: "Models",
    skills: "Skills",
    noProviders: "No providers configured.",
    noModels: "No models available.",
    noSkills: "No skills available.",
    id: "ID",
    auth: "Auth",
    api: "API",
    kind: "Kind",
    context: "Context",
    output: "Output",
    tokens: "tokens",
    mIn: "M in",
    mOut: "M out",
    caps: {
      chat: "Chat",
      tools: "Tools",
      streaming: "Stream",
      structured_output: "JSON",
      vision: "Vision",
    },
  },
  createModal: {
    title: "New Session",
    hint: "Choose the session mode:",
    chatLabel: "Chat",
    chatDesc: "Simple conversation, no tools.",
    agentLabel: "Agent",
    agentDesc: "Can call tools for tasks.",
    assistantLabel: "Assistant",
    assistantDesc: "Autonomous progression.",
  },
  themeSwitcher: {
    light: "Light",
    dark: "Dark",
    system: "System",
  },
  toolCall: {
    arguments: "Arguments",
  },
  drawer: {
    sessions: "Sessions",
    info: "Info",
  },
  topicPanel: {
    topics: "Topics",
    all: "All",
    create: "New Topic",
    createTitle: "Create Topic",
    createName: "Name",
    createDesc: "Description",
    createSubmit: "Create",
    creating: "Creating...",
    noTopics: "No topics yet.",
    deleteConfirm: "Are you sure you want to delete this topic?",
    cancel: "Cancel",
    confirm: "Delete",
    pause: "Pause",
    resume: "Resume",
    progress: "Progress",
    scopeItems: "Scope Items",
    addScopeItem: "Add Item",
    scopeGoal: "Goal",
    scopeContract: "Done Contract",
    scopeAdd: "Add",
    scopeStatusPending: "Pending",
    scopeStatusDone: "Done",
    status: "Status",
    description: "Description",
    updated: "Updated",
    sessionId: "Session",
    name: "Name",
  },
  pollerPanel: {
    poller: "Poller",
    status: "Status",
    running: "Running",
    paused: "Paused",
    tickCount: "Ticks",
    taskCount: "Tasks",
    interval: "Interval",
    pause: "Pause",
    resume: "Resume",
    trigger: "Trigger",
    triggering: "Triggering...",
    noPoller: "Poller not available.",
    pendingTrigger: "Pending Trigger",
  },
  neuronPanel: {
    neurons: "Neurons",
    list: "Neuron List",
    detail: "Neuron Detail",
    network: "Network View",
    noNeurons: "No neurons yet.",
    weight: "Weight",
    systemType: "System Type",
    description: "Description",
    content: "Content",
    toolIds: "Tool IDs",
    createdAt: "Created",
    updatedAt: "Updated",
    edit: "Edit",
    save: "Save",
    saving: "Saving...",
    cancel: "Cancel",
    connections: "Connections",
    source: "Source",
    target: "Target",
    connectionWeight: "Weight",
    networkTitle: "Network (depth: {depth})",
    networkDepth: "Depth",
    viewModeGraph: "Graph",
    viewModeTree: "Tree",
    edgeWeight: "Edge",
    depthLabel: "Depth",
    edgeTypeLabel: "Edge",
    edgeFloating: "Auto",
    edgeBezier: "Bezier",
    edgeSmoothstep: "Smooth",
    edgeStep: "Step",
    edgeStraight: "Straight",
    back: "Back",
    viewNetwork: "View Network",
    jumpTo: "View",
    loading: "Loading...",
    search: "Search neurons...",
    filterAll: "All",
    connectionsCount: "{count} links",
    emptyTitle: "No neurons yet",
    emptyHint: "Talk in Assistant mode to grow your neuron network.",
    startAssistant: "Start in Assistant mode",
    focusNode: "Focus",
    drawerTitle: "Neuron",
    close: "Close",
  },
};

export const zh: Translations = {
  locale: { label: "中文" },
  common: {
    send: "发送",
    sending: "发送中...",
    thinking: "思考中...",
    noModel: "未选择模型",
    session: "会话",
  },
  statusBar: {
    appName: "智能体应用",
    toggleSidebar: "切换左栏",
    toggleInfo: "切换右栏",
    togglePanel: "切换底栏",
  },
  sessionList: {
    title: "会话列表",
    empty: "暂无会话。",
    create: "创建一个",
    msgs: "条消息",
  },
  chatArea: {
    emptyTitle: "开始对话",
    emptyDesc: "在下方输入消息开始对话",
    chatInputPlaceholder: "输入消息... (Enter 发送, Shift+Enter 换行)",
  },
  chatMessage: {
    you: "你",
    assistant: "助手",
    system: "系统",
  },
  sidePanel: {
    providers: "服务商",
    models: "模型",
    skills: "技能",
    noProviders: "未配置服务商。",
    noModels: "无可用模型。",
    noSkills: "无可用技能。",
    id: "ID",
    auth: "认证",
    api: "API",
    kind: "类型",
    context: "上下文",
    output: "输出",
    tokens: "tokens",
    mIn: "M 输入",
    mOut: "M 输出",
    caps: {
      chat: "对话",
      tools: "工具",
      streaming: "流式",
      structured_output: "JSON",
      vision: "视觉",
    },
  },
  createModal: {
    title: "新建会话",
    hint: "选择会话模式：",
    chatLabel: "对话",
    chatDesc: "简单对话，不调用工具。",
    agentLabel: "智能体",
    agentDesc: "可调用工具完成任务。",
    assistantLabel: "助手",
    assistantDesc: "自主推进模式。",
  },
  themeSwitcher: {
    light: "浅色",
    dark: "深色",
    system: "跟随系统",
  },
  toolCall: {
    arguments: "参数",
  },
  drawer: {
    sessions: "会话列表",
    info: "信息",
  },
  topicPanel: {
    topics: "课题",
    all: "全部",
    create: "新建课题",
    createTitle: "创建课题",
    createName: "名称",
    createDesc: "描述",
    createSubmit: "创建",
    creating: "创建中...",
    noTopics: "暂无课题。",
    deleteConfirm: "确定要删除此课题吗？",
    cancel: "取消",
    confirm: "删除",
    pause: "暂停",
    resume: "恢复",
    progress: "进度",
    scopeItems: "范围项",
    addScopeItem: "添加项",
    scopeGoal: "目标",
    scopeContract: "完成条件",
    scopeAdd: "添加",
    scopeStatusPending: "待办",
    scopeStatusDone: "完成",
    status: "状态",
    description: "描述",
    updated: "更新于",
    sessionId: "会话",
    name: "名称",
  },
  pollerPanel: {
    poller: "轮询器",
    status: "状态",
    running: "运行中",
    paused: "已暂停",
    tickCount: "Tick 数",
    taskCount: "任务数",
    interval: "间隔",
    pause: "暂停",
    resume: "恢复",
    trigger: "触发",
    triggering: "触发中...",
    noPoller: "Poller 不可用。",
    pendingTrigger: "待触发",
  },
  neuronPanel: {
    neurons: "神经元",
    list: "神经元列表",
    detail: "神经元详情",
    network: "网络视图",
    noNeurons: "暂无神经元。",
    weight: "权重",
    systemType: "系统类型",
    description: "描述",
    content: "内容",
    toolIds: "工具 ID",
    createdAt: "创建于",
    updatedAt: "更新于",
    edit: "编辑",
    save: "保存",
    saving: "保存中...",
    cancel: "取消",
    connections: "连接",
    source: "来源",
    target: "目标",
    connectionWeight: "权重",
    networkTitle: "网络视图（深度: {depth}）",
    networkDepth: "深度",
    viewModeGraph: "图",
    viewModeTree: "树",
    edgeWeight: "边权",
    depthLabel: "深度",
    edgeTypeLabel: "连线",
    edgeFloating: "自动",
    edgeBezier: "曲线",
    edgeSmoothstep: "圆角",
    edgeStep: "直角",
    edgeStraight: "直线",
    back: "返回",
    viewNetwork: "查看网络",
    jumpTo: "查看",
    loading: "加载中...",
    search: "搜索神经元...",
    filterAll: "全部",
    connectionsCount: "{count} 条连接",
    emptyTitle: "暂无神经元",
    emptyHint: "在 Assistant 模式下对话，让神经元网络生长。",
    startAssistant: "进入 Assistant 模式",
    focusNode: "聚焦",
    drawerTitle: "神经元",
    close: "关闭",
  },
};

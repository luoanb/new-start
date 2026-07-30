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
};

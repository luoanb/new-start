export type Translations = {
  locale: { label: string };
  common: {
    send: string;
    sending: string;
    thinking: string;
    noModel: string;
    session: string;
    selected: string;
    selectAll: string;
    clickToCopy: string;
    copied: string;
    copyFailed: string;
    mainEmpty: string;
    newPane: string;
    confirm: string;
    cancel: string;
  };
  statusBar: {
    appName: string;
    toggleSidebar: string;
    toggleInfo: string;
    togglePanel: string;
    toggleNav: string;
  };
  sessionList: {
    title: string;
    empty: string;
    emptyHint: string;
    create: string;
    newButton: string;
    msgs: string;
    newSession: string;
    yesterday: string;
    running: string;
    copyId: string;
    closeSession: string;
    collapseSidebar: string;
    expandSidebar: string;
  };
  chatArea: {
    emptyTitle: string;
    emptyDesc: string;
    chatInputPlaceholder: string;
    stop: string;
  };
  chatMessage: {
    you: string;
    assistant: string;
    system: string;
    tool: string;
    compaction: string;
    nudge: string;
    context: string;
    copy: string;
    rate: string;
    copied: string;
  };
  sidePanel: {
    providers: string;
    models: string;
    noProviders: string;
    noModels: string;
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
    systemLabel: string;
    systemDesc: string;
  };
  connectDialog: {
    title: string;
    mode: string;
    modeLocal: string;
    modeLocalHint: string;
    modeRemote: string;
    modeRemoteHint: string;
    address: string;
    addressPlaceholder: string;
    token: string;
    tokenHint: string;
    test: string;
    testing: string;
    reachable: string;
    unreachable: string;
    needUrl: string;
    save: string;
    saving: string;
    cancel: string;
    switchFailed: string;
    lockedHint: string;
  };
  neuronListPanel: {
    title: string;
    search: string;
    kindAll: string;
    kindSystem: string;
    kindNormal: string;
    multiSelect: string;
    create: string;
    edit: string;
    launch: string;
    launchHint: string;
    loadMore: string;
    noMore: string;
    empty: string;
    loading: string;
  };
  neuronEditor: {
    systemType: string;
    systemTypeUnbound: string;
    bind: string;
    rebind: string;
    unbind: string;
    bindPlaceholder: string;
    bindConfirmTitle: string;
    bindConfirmBody: string;
    unbindConfirmTitle: string;
    unbindConfirmBody: string;
    behavior: string;
    saveBehavior: string;
    selection: string;
    tools: string;
    insertId: string;
    none: string;
    fixed: string;
    neighborhood: string;
    global: string;
    globalLimit: string;
    toolNone: string;
    toolFromNeuron: string;
    toolAllowlist: string;
    allowlistHint: string;
    confirm: string;
    cancel: string;
    operationFailed: string;
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
  toolCall: {
    arguments: string;
  };
  thinking: {
    title: string;
  };
  toolResult: {
    executed: string;
    timedOut: string;
    empty: string;
    stdout: string;
    stderr: string;
  };
  terminal: {
    newTab: string;
    closeTab: string;
    exited: string;
    connecting: string;
    disconnected: string;
    empty: string;
    exitCode: string;
    initFailed: string;
    spawnFailed: string;
    writeFailed: string;
  };
  logPanel: {
    verbosity: string;
    minLevel: string;
    target: string;
    keyword: string;
    clear: string;
    file: string;
    empty: string;
    targetPlaceholder: string;
    keywordPlaceholder: string;
    initFailed: string;
    setLevelFailed: string;
    clearFailed: string;
  };
  drawer: {
    sessions: string;
    info: string;
    panel: string;
  };
  topicPanel: {
    topics: string;
    all: string;
    filterActive: string;
    filterDone: string;
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
    openConversation: string;
    scopeItems: string;
    addScopeItem: string;
    scopeGoal: string;
    scopeContract: string;
    scopeAdd: string;
    scopeStatusPending: string;
    scopeStatusDone: string;
    scopeStatusBlocked: string;
    status: string;
    description: string;
    updated: string;
    sessionId: string;
    name: string;
    topicStatus: Record<string, string>;
    createFailed: string;
    pauseFailed: string;
    resumeFailed: string;
    deleteFailed: string;
    addScopeFailed: string;
    completeScopeFailed: string;
    deleteScopeFailed: string;
  };
  pollerPanel: {
    poller: string;
    status: string;
    running: string;
    paused: string;
    tickCount: string;
    taskCount: string;
    interval: string;
    parallelism: string;
    parallelismHint: string;
    save: string;
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
    delta: string;
    apply: string;
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
    posRight: string;
    posBottom: string;
    create: string;
    createTitle: string;
    createOrphan: string;
    createDownstream: string;
    createSource: string;
    createSourcePlaceholder: string;
    createDescLabel: string;
    createDescPlaceholder: string;
    createContentLabel: string;
    createContentPlaceholder: string;
    createDescRequired: string;
    createSourceRequired: string;
    createConfirm: string;
    creating: string;
    createDownstreamFromHere: string;
    createToolIdsLabel: string;
    createToolIdsPlaceholder: string;
    noToolsAvailable: string;
    id: string;
    copy: string;
    copied: string;
    layoutLabel: string;
    layoutForce: string;
    layoutLayered: string;
    setAsSeed: string;
  };
  toolPanel: {
    title: string;
    reload: string;
    editConfig: string;
    loading: string;
    mcpSection: string;
    toolsSection: string;
    httpToolsSection: string;
    commandToolsSection: string;
    toolsCount: string;
    noMcpServers: string;
    noTools: string;
    status: Record<string, string>;
    modalTitle: string;
    modalAria: string;
    close: string;
    loadingConfig: string;
    add: string;
    delete: string;
    emptyMcp: string;
    emptyHttp: string;
    emptyCommand: string;
    name: string;
    transport: string;
    method: string;
    command: string;
    args: string;
    url: string;
    timeoutMs: string;
    desc: string;
    template: string;
    disabled: string;
    tag: string;
    transportHint: string;
    httpUrlHint: string;
    commandHint: string;
    saveHint: string;
    optional: string;
    descPlaceholder: string;
    cancel: string;
    save: string;
    saving: string;
    loadFailed: string;
    reassembleFailed: string;
    loadListFailed: string;
  };
  views: {
    sessions: string;
    providers: string;
    models: string;
    topics: string;
    poller: string;
    tools: string;
    logs: string;
    terminal: string;
    chat: string;
    neurons: string;
    toolEditor: string;
    neuronsList: string;
    providersModels: string;
    providerManager: string;
    files: string;
    fileEditor: string;
    git: string;
    gitDiff: string;
    search: string;
    hookJudgements: string;
  };
  searchPanel: {
    placeholder: string;
    search: string;
    indexing: string;
    noWorkspace: string;
    noQuery: string;
    empty: string;
    openInEditor: string;
    loadFailed: string;
    results: string;
    blocks: string;
  };
  judgement: {
    status: {
      pending: string;
      ok: string;
      retriedOk: string;
      downgraded: string;
    };
    hookType: string;
    statusLabel: string;
    conversation: string;
    all: string;
    attempts: string;
    durationMs: string;
    model: string;
    payload: string;
    attemptsDetail: string;
    rawResponse: string;
    decision: string;
    error: string;
    locate: string;
    empty: string;
    noMatch: string;
    running: string;
    refresh: string;
    tooltip: {
      pending: string;
      ok: string;
      retriedOk: string;
      downgraded: string;
    };
    loadFailed: string;
  };
  hook: {
    completeScope: string;
    matchTopic: string;
    reviseTopic: string;
    scoreFeedback: string;
  };
  fileExplorer: {
    addWorkspace: string;
    addWorkspaceHint: string;
    selectWorkspace: string;
    addWorkspaceInputPlaceholder: string;
    addWorkspaceInputConfirm: string;
    addWorkspaceBrowse: string;
    workspaceActions: string;
    editIgnore: string;
    deleteWorkspace: string;
    deleteWorkspaceConfirm: string;
    ignoreTitle: string;
    ignoreHint: string;
    ignoreSave: string;
    ignoreCancel: string;
    refresh: string;
    newFile: string;
    newFolder: string;
    rename: string;
    delete: string;
    move: string;
    moveTargetHint: string;
    moveConfirm: string;
    copyPath: string;
    open: string;
    empty: string;
    noWorkspace: string;
    loading: string;
    loadFailed: string;
    invalidName: string;
    operationFailed: string;
    copied: string;
  };
  fileEditor: {
    save: string;
    saving: string;
    saved: string;
    saveFailed: string;
    closeUnsavedTitle: string;
    closeUnsavedBody: string;
    discard: string;
    conflictTitle: string;
    conflictBody: string;
    overwrite: string;
    cancel: string;
    fileMissing: string;
    workspaceMismatch: string;
    binaryRejected: string;
    loadFailed: string;
    loading: string;
    loadedLines: string;
    loadMore: string;
  };
  git: {
    repo: string;
    repoPlaceholder: string;
    noRepos: string;
    branch: string;
    summary: string;
    groupStaged: string;
    groupChanges: string;
    groupConflicted: string;
    groupLog: string;
    logEmpty: string;
    logLoading: string;
    logMore: string;
    openCommitDiff: string;
    /** git 状态码 → 精炼说明（徽标 hover 提示，按 trim 后的码匹配） */
    status: Record<string, string>;
    /** 单字符状态含义（通用模板兜底拆解） */
    states: Record<string, string>;
    /** 双字符码兜底模板：暂存区 / 工作区 */
    statusTemplate: string;
    stage: string;
    unstage: string;
    stageAll: string;
    unstageAll: string;
    filesCount: string;
    commitPlaceholder: string;
    commit: string;
    committing: string;
    nothingToCommit: string;
    branches: string;
    stash: string;
    stashCreate: string;
    stashApply: string;
    stashDrop: string;
    notRepo: string;
    clean: string;
    pull: string;
    push: string;
    discard: string;
    editIgnore: string;
    refresh: string;
    loadFailed: string;
    operationFailed: string;
    checkoutFailed: string;
    commitConfirmTitle: string;
    commitConfirmBody: string;
    pushConfirmTitle: string;
    pushConfirmBody: string;
    pullConfirmTitle: string;
    pullConfirmBody: string;
    discardConfirmTitle: string;
    discardConfirmBody: string;
    checkoutConfirmBody: string;
    stashApplyConfirmTitle: string;
    stashApplyConfirmBody: string;
    stashDropConfirmTitle: string;
    stashDropConfirmBody: string;
    confirmDiscard: string;
    dangerousWrites: string;
    rangeStaged: string;
    rangeUnstaged: string;
    rangeBoth: string;
    prevHunk: string;
    nextHunk: string;
    hunkCount: string;
    acceptOurs: string;
    acceptTheirs: string;
    acceptBoth: string;
    blame: string;
    blameLoading: string;
    diffEmpty: string;
    untrackedHint: string;
    binaryDiff: string;
    missingFile: string;
  };
  providersModelsPanel: {
    create: string;
    expand: string;
    collapse: string;
    delete: string;
    deleteConfirm: string;
    deleteGo: string;
    cancel: string;
  };
  providerManager: {
    modalTitle: string;
    loading: string;
    providers: string;
    addProvider: string;
    untitled: string;
    disabled: string;
    builtin: string;
    noProviders: string;
    providerFields: string;
    id: string;
    displayName: string;
    kind: string;
    apiBase: string;
    authEnv: string;
    apiKey: string;
    apiKeyMasked: string;
    apiKeyPlaceholder: string;
    enabled: string;
    models: string;
    addModel: string;
    noModels: string;
    contextWindow: string;
    maxOutput: string;
    priceIn: string;
    priceOut: string;
    deleteModel: string;
    deleteProvider: string;
    builtinDeleteHint: string;
    disableProvider: string;
    customDeleteHint: string;
    selectProvider: string;
    defaultsHint: string;
    noDefaults: string;
    cancel: string;
    save: string;
    saving: string;
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
    selected: "Selected: {count}",
    selectAll: "Select all",
    clickToCopy: "Click to copy",
    copied: "Copied",
    copyFailed: "Copy failed",
    mainEmpty: "Open a panel from the activity bar to get started.",
    newPane: "New Pane",
    confirm: "Confirm",
    cancel: "Cancel",
  },
  statusBar: {
    appName: "Pulsar",
    toggleSidebar: "Toggle sidebar",
    toggleInfo: "Toggle info panel",
    togglePanel: "Toggle bottom panel",
    toggleNav: "Toggle navigation bar",
  },
  sessionList: {
    title: "Sessions",
    empty: "No sessions yet.",
    emptyHint: "Start a conversation to see it here",
    create: "Create one",
    newButton: "New session",
    msgs: "msgs",
    newSession: "New session",
    yesterday: "Yesterday",
    running: "Running",
    copyId: "Copy session ID",
    closeSession: "Close session",
    collapseSidebar: "Collapse sidebar",
    expandSidebar: "Expand sidebar",
  },
  chatArea: {
    emptyTitle: "Start a conversation",
    emptyDesc: "Send a message below to begin",
    chatInputPlaceholder: "Type a message... (Enter to send, Shift+Enter for new line)",
    stop: "Stop",
  },
  chatMessage: {
    you: "You",
    assistant: "Assistant",
    system: "system",
    tool: "tool",
    compaction: "summary",
    nudge: "polling advance",
    context: "role context",
    copy: "Copy",
    rate: "Rate",
    copied: "Copied",
  },
  sidePanel: {
    providers: "Providers",
    models: "Model Manager",
    noProviders: "No providers configured.",
    noModels: "No models available.",
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
    systemLabel: "System",
    systemDesc: "Assistant with system tools.",
  },
  connectDialog: {
    title: "Connection",
    mode: "Mode",
    modeLocal: "Local",
    modeLocalHint: "Access the app via Tauri IPC",
    modeRemote: "Remote",
    modeRemoteHint: "Access the embedded service over the network",
    address: "Server address",
    addressPlaceholder: "http://127.0.0.1:8787",
    token: "Access token (optional)",
    tokenHint: "Not required when the backend whitelist is empty",
    test: "Test",
    testing: "Testing...",
    reachable: "Server reachable",
    unreachable: "Cannot connect",
    needUrl: "URL is required in remote mode",
    save: "Save & switch",
    saving: "Switching...",
    cancel: "Cancel",
    switchFailed: "Switch failed: {error}",
    lockedHint: "Connection failed. Enter a valid server address to continue.",
  },
  neuronListPanel: {
    title: "Neurons",
    search: "Search neurons...",
    kindAll: "All",
    kindSystem: "System",
    kindNormal: "Normal",
    multiSelect: "Multi-select",
    create: "New",
    edit: "Edit",
    launch: "Launch",
    launchHint: "Open a session with this neuron",
    loadMore: "Load more ↓",
    noMore: "No more",
    empty: "No neurons yet.",
    loading: "Loading...",
  },
  neuronEditor: {
    systemType: "System Type",
    systemTypeUnbound: "Unbound",
    bind: "Bind",
    rebind: "Re-bind",
    unbind: "Unbind",
    bindPlaceholder: "e.g. session.my_spec",
    bindConfirmTitle: "Bind system type",
    bindConfirmBody: "Bind this neuron as system type “{type}”? Behavior controls will appear after binding.",
    unbindConfirmTitle: "Unbind system type",
    unbindConfirmBody: "Remove system type and turn this back into a normal neuron? Behavior controls will be hidden.",
    behavior: "Behavior",
    saveBehavior: "Save behavior",
    selection: "Selection",
    tools: "Tools",
    insertId: "Contract manual",
    none: "None",
    fixed: "Fixed",
    neighborhood: "Neighborhood",
    global: "Global",
    globalLimit: "Global limit",
    toolNone: "None",
    toolFromNeuron: "From neuron",
    toolAllowlist: "Allowlist",
    allowlistHint: "Comma-separated tool ids",
    confirm: "Confirm",
    cancel: "Cancel",
    operationFailed: "Operation failed",
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
  toolCall: {
    arguments: "Arguments",
  },
  thinking: {
    title: "Thinking",
  },
  toolResult: {
    executed: "Command executed",
    timedOut: "timed out",
    empty: "No output",
    stdout: "stdout",
    stderr: "stderr",
  },
  terminal: {
    newTab: "New terminal",
    closeTab: "Close terminal",
    exited: "Exited",
    connecting: "Connecting to terminal server…",
    disconnected: "Terminal connection lost. Retrying…",
    empty: "No terminal. Click + to create one.",
    exitCode: "process exited with code {code}",
    initFailed: "Terminal init failed: {error}",
    spawnFailed: "Spawn failed: {error}",
    writeFailed: "Write failed: {error}",
  },
  logPanel: {
    verbosity: "Verbosity",
    minLevel: "Min level",
    target: "Target",
    keyword: "Keyword",
    clear: "Clear",
    file: "file",
    empty: "No log entries match the current filters.",
    targetPlaceholder: "neuron / gateway…",
    keywordPlaceholder: "phase / error_code…",
    initFailed: "Logs init failed: {error}",
    setLevelFailed: "Set level failed: {error}",
    clearFailed: "Clear failed: {error}",
  },
  drawer: {
    sessions: "Sessions",
    info: "Info",
    panel: "Panel",
  },
  topicPanel: {
    topics: "Topics",
    all: "All",
    filterActive: "Active",
    filterDone: "Done",
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
    openConversation: "Open conversation",
    scopeItems: "Scope Items",
    addScopeItem: "Add Item",
    scopeGoal: "Goal",
    scopeContract: "Done Contract",
    scopeAdd: "Add",
    scopeStatusPending: "Pending",
    scopeStatusDone: "Done",
    scopeStatusBlocked: "Waiting user",
    status: "Status",
    description: "Description",
    updated: "Updated",
    sessionId: "Session",
    name: "Name",
    topicStatus: {
      todo: "Todo",
      in_progress: "In Progress",
      paused: "Paused",
      done: "Done",
      cancelled: "Cancelled",
      waiting_user: "Waiting for user",
      wrapping_up: "Wrapping up",
    },
    createFailed: "Create failed: {error}",
    pauseFailed: "Pause failed: {error}",
    resumeFailed: "Resume failed: {error}",
    deleteFailed: "Delete failed: {error}",
    addScopeFailed: "Add scope item failed: {error}",
    completeScopeFailed: "Complete scope item failed: {error}",
    deleteScopeFailed: "Delete scope item failed: {error}",
  },
  pollerPanel: {
    poller: "Poller",
    status: "Status",
    running: "Running",
    paused: "Paused",
    tickCount: "Ticks",
    taskCount: "Tasks",
    interval: "Interval",
    parallelism: "Parallelism",
    parallelismHint: "Topics advanced per poll round (1–8); saved & applied immediately.",
    save: "Save",
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
    delta: "Delta",
    apply: "Apply",
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
    posRight: "Dock right",
    posBottom: "Dock bottom",
    create: "New Neuron",
    createTitle: "Create Neuron",
    createOrphan: "Orphan",
    createDownstream: "Downstream",
    createSource: "Upstream neuron",
    createSourcePlaceholder: "Select upstream neuron...",
    createDescLabel: "Description",
    createDescPlaceholder: "Short description of the neuron",
    createContentLabel: "Content",
    createContentPlaceholder: "Knowledge / content of the neuron (optional)",
    createDescRequired: "Description is required.",
    createSourceRequired: "Please select an upstream neuron.",
    createConfirm: "Create",
    creating: "Creating...",
    createDownstreamFromHere: "Create downstream neuron",
    createToolIdsLabel: "Tools",
    createToolIdsPlaceholder: "Select tools this neuron can use (leave empty for none)",
    noToolsAvailable: "No tools available",
    id: "ID",
    copy: "Copy",
    copied: "Copied",
    layoutLabel: "Layout",
    layoutForce: "Force-directed",
    layoutLayered: "Layered",
    setAsSeed: "Set as seed",
  },
  toolPanel: {
    title: "Tools",
    reload: "Reload",
    editConfig: "Edit config",
    loading: "Loading…",
    mcpSection: "MCP Servers",
    toolsSection: "Tools",
    httpToolsSection: "HTTP Tools",
    commandToolsSection: "Command Tools",
    toolsCount: "{count} tools",
    noMcpServers: "No MCP servers. Click \"Edit config\" in the top-right to add one.",
    noTools: "No tools available",
    status: {
      connecting: "Connecting",
      connected: "Connected",
      failed: "Failed",
      disabled: "Disabled",
    },
    modalTitle: "Tool configuration",
    modalAria: "Edit tool configuration",
    close: "Close",
    loadingConfig: "Loading config…",
    add: "Add",
    delete: "Delete",
    emptyMcp: "No MCP servers",
    emptyHttp: "No HTTP tools",
    emptyCommand: "No command tools",
    name: "Name",
    transport: "Transport",
    method: "Method",
    command: "Command",
    args: "Args (comma separated)",
    url: "URL",
    timeoutMs: "Timeout (ms)",
    desc: "Description",
    template: "Template (command)",
    disabled: "Disabled",
    tag: "Tag",
    transportHint: "stdio requires command; http requires URL",
    httpUrlHint: "Fixed endpoint; {query} filled by the model",
    commandHint: "Command passes safety rails: denylist / timeout / concurrency",
    saveHint: "Saved immediately: writes JSON and reassembles tools",
    optional: "Optional",
    descPlaceholder: "Tool description",
    cancel: "Cancel",
    save: "Save",
    saving: "Saving…",
    loadFailed: "Failed to load config",
    reassembleFailed: "Reassemble failed: {error}",
    loadListFailed: "Failed to load tools: {error}",
  },
  views: {
    sessions: "Sessions",
    providers: "Providers",
    models: "Models",
    topics: "Topics",
    poller: "Poller",
    tools: "Tools",
    logs: "Logs",
    terminal: "Terminal",
    chat: "Chat",
    neurons: "Neurons",
    toolEditor: "Tool config",
    neuronsList: "Neurons",
    providersModels: "Model Manager",
    providerManager: "Model Manager",
    files: "Files",
    fileEditor: "File Editor",
    git: "Git",
    gitDiff: "Diff",
    search: "Search",
    hookJudgements: "Hook Judgements",
  },
  searchPanel: {
    placeholder: "Search symbols, functions, types…",
    search: "Search",
    indexing: "Indexing workspace… first search may take a while",
    noWorkspace: "No workspace added",
    noQuery: "Enter keywords to search",
    empty: "No matching blocks",
    openInEditor: "Open in editor",
    loadFailed: "Search failed: {error}",
    results: "{n} results",
    blocks: "{n} blocks indexed · {ms} ms",
  },
  judgement: {
    status: {
      pending: "Pending",
      ok: "OK",
      retriedOk: "Retried OK",
      downgraded: "Downgraded",
    },
    hookType: "Hook",
    statusLabel: "Status",
    conversation: "Conversation",
    all: "All",
    attempts: "Attempts",
    durationMs: "Duration (ms)",
    model: "Model",
    payload: "Judgement Input",
    attemptsDetail: "Attempts Detail",
    rawResponse: "Raw Output",
    decision: "Decision",
    error: "Error",
    locate: "Locate in conversation",
    empty: "No hook judgement records yet",
    noMatch: "No matching records",
    running: "Judging · {hook}",
    refresh: "Refresh",
    tooltip: {
      pending: "Judgement in progress",
      ok: "Succeeded on first try",
      retriedOk: "Failed once, succeeded on retry",
      downgraded: "Retries failed; fell back to neutral default",
    },
    loadFailed: "Failed to load: {error}",
  },
  hook: {
    completeScope: "Complete Scope",
    matchTopic: "Match Topic",
    reviseTopic: "Revise Topic",
    scoreFeedback: "Score Feedback",
  },
  fileExplorer: {
    addWorkspace: "Add Workspace",
    addWorkspaceHint: "Pick a project folder to manage files",
    selectWorkspace: "Select workspace",
    addWorkspaceInputPlaceholder: "Enter absolute directory path",
    addWorkspaceInputConfirm: "Add",
    addWorkspaceBrowse: "Browse…",
    workspaceActions: "Workspace actions",
    editIgnore: "Edit ignore rules",
    deleteWorkspace: "Remove workspace",
    deleteWorkspaceConfirm: "Only the config entry is removed; disk files are kept. Remove this workspace?",
    ignoreTitle: "Ignore rules",
    ignoreHint: "One per line (glob or name, e.g. node_modules, *.log). Defaults hide .git / node_modules / target / dist / .pulsar",
    ignoreSave: "Save",
    ignoreCancel: "Cancel",
    refresh: "Refresh",
    newFile: "New file",
    newFolder: "New folder",
    rename: "Rename",
    delete: "Delete",
    move: "Move…",
    moveTargetHint: "Move to: pick a target folder",
    moveConfirm: "Move",
    copyPath: "Copy path",
    open: "Open",
    empty: "This folder is empty",
    noWorkspace: "No workspace yet",
    loading: "Loading…",
    loadFailed: "Failed to load",
    invalidName: "Invalid name",
    operationFailed: "Operation failed: {error}",
    copied: "Copied",
  },
  fileEditor: {
    save: "Save",
    saving: "Saving…",
    saved: "Saved",
    saveFailed: "Save failed: {error}",
    closeUnsavedTitle: "Close unsaved file?",
    closeUnsavedBody: "This file has unsaved changes. Close anyway?",
    discard: "Discard changes",
    conflictTitle: "File changed on disk",
    conflictBody: "The file changed on disk since it was opened. Overwriting will lose external changes. Continue?",
    overwrite: "Overwrite",
    cancel: "Cancel",
    fileMissing: "File missing or moved",
    workspaceMismatch: "Workspace switched; switch back to its workspace to edit this file",
    binaryRejected: "Cannot open binary file",
    loadFailed: "Open failed: {error}",
    loading: "Loading…",
    loadedLines: "Loaded {loaded}/{total} lines",
    loadMore: "Scroll to bottom to load more",
  },
  git: {
    repo: "Repository",
    repoPlaceholder: "Select repository",
    noRepos: "No git repository found",
    branch: "Branch",
    summary: "{staged} staged · {changes} changed",
    groupStaged: "Staged",
    groupChanges: "Changes",
    groupConflicted: "Conflicts",
    groupLog: "Commits",
    logEmpty: "No commits",
    logLoading: "Loading…",
    logMore: "Load more",
    openCommitDiff: "Open in main area",
    status: {
      "??": "Untracked",
      M: "Modified",
      A: "Added",
      D: "Deleted",
      R: "Renamed",
      U: "Conflict",
      MM: "Staged changes + further unstaged changes",
      AM: "Staged new file + further unstaged changes",
      AD: "Staged new file, then deleted in working tree",
      DD: "Both deleted",
      UU: "Both modified (conflict)",
    },
    states: {
      M: "Modified",
      A: "Added",
      D: "Deleted",
      R: "Renamed",
      C: "Copied",
      U: "Conflict",
      "?": "Untracked",
      T: "Type changed",
      "!": "Ignored",
      " ": "Unchanged",
    },
    statusTemplate: "Staged: {x} / Changes: {y}",
    stage: "Stage",
    unstage: "Unstage",
    stageAll: "Stage All",
    unstageAll: "Unstage All",
    filesCount: "{n} files",
    commitPlaceholder: "Commit message…",
    commit: "Commit",
    committing: "Committing…",
    nothingToCommit: "Nothing to commit",
    branches: "Branches",
    stash: "Stash",
    stashCreate: "Stash…",
    stashApply: "Apply",
    stashDrop: "Drop",
    notRepo: "Current workspace is not a git repository",
    clean: "Working tree clean",
    pull: "Pull",
    push: "Push",
    discard: "Discard changes",
    editIgnore: "Edit repo ignore rules",
    refresh: "Refresh",
    loadFailed: "Failed to load: {error}",
    operationFailed: "Operation failed: {error}",
    checkoutFailed: "Checkout failed: {error}",
    commitConfirmTitle: "Commit changes",
    commitConfirmBody: "Commit {n} staged file(s)?",
    pushConfirmTitle: "Push",
    pushConfirmBody: "Push {branch} (ahead by {ahead})?",
    pullConfirmTitle: "Pull",
    pullConfirmBody: "Pull latest and merge into {branch}? Local changes may conflict.",
    discardConfirmTitle: "Discard changes",
    discardConfirmBody: "This will permanently discard changes in {n} file(s). Continue?",
    checkoutConfirmBody: "Switch to {target}? Uncommitted changes may be overwritten.",
    stashApplyConfirmTitle: "Apply stash",
    stashApplyConfirmBody: "Apply this stash? This may cause conflicts.",
    stashDropConfirmTitle: "Drop stash",
    stashDropConfirmBody: "Drop this stash? This cannot be undone.",
    confirmDiscard: "Discard",
    dangerousWrites: "Allow dangerous writes (reset --hard / checkout overwrite)",
    rangeStaged: "Staged (vs HEAD)",
    rangeUnstaged: "Changes (vs index)",
    rangeBoth: "All changes",
    prevHunk: "Previous hunk",
    nextHunk: "Next hunk",
    hunkCount: "{current}/{total}",
    acceptOurs: "Accept ours",
    acceptTheirs: "Accept theirs",
    acceptBoth: "Accept both",
    blame: "Blame",
    blameLoading: "Analyzing blame…",
    diffEmpty: "No changes",
    untrackedHint: "Untracked file",
    binaryDiff: "Binary file, diff not available",
    missingFile: "File not found",
  },
  providersModelsPanel: {
    create: "New provider",
    expand: "Expand",
    collapse: "Collapse",
    delete: "Delete",
    deleteConfirm: "Delete this provider? Disabled built-ins are hidden from the list.",
    deleteGo: "Delete",
    cancel: "Cancel",
  },
  providerManager: {
    modalTitle: "Model Manager",
    loading: "Loading config…",
    providers: "Providers",
    addProvider: "Add provider",
    untitled: "Untitled",
    disabled: "Disabled",
    builtin: "Built-in",
    noProviders: "No providers",
    providerFields: "Provider",
    id: "ID",
    displayName: "Display name",
    kind: "Kind",
    apiBase: "API base",
    authEnv: "Auth env",
    apiKey: "API key",
    apiKeyMasked: "Configured (masked); type a new value to replace it",
    apiKeyPlaceholder: "sk-... (leave empty to skip)",
    enabled: "Enabled",
    models: "Models",
    addModel: "Add model",
    noModels: "No models configured",
    contextWindow: "Context (tokens)",
    maxOutput: "Max output",
    priceIn: "Price in ($/M)",
    priceOut: "Price out ($/M)",
    deleteModel: "Delete model",
    deleteProvider: "Danger zone",
    builtinDeleteHint: "Built-in providers cannot be physically removed. Disabling hides it from the list; re-enable it anytime in this editor.",
    disableProvider: "Disable provider",
    customDeleteHint: "Deleting a custom provider removes it from config.json entirely.",
    selectProvider: "Select a provider on the left to edit.",
    defaultsHint: "Default model: {provider} / {model}",
    noDefaults: "No default model configured.",
    cancel: "Cancel",
    save: "Save",
    saving: "Saving…",
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
    selected: "已选 {count}",
    selectAll: "全选",
    clickToCopy: "点击复制",
    copied: "已复制",
    copyFailed: "复制失败",
    mainEmpty: "从左侧入口打开一个面板开始使用",
    newPane: "新建分栏",
    confirm: "确认",
    cancel: "取消",
  },
  statusBar: {
    appName: "星脉",
    toggleSidebar: "切换左栏",
    toggleInfo: "切换右栏",
    togglePanel: "切换底栏",
    toggleNav: "切换导航栏",
  },
  sessionList: {
    title: "会话",
    empty: "暂无会话。",
    emptyHint: "开始对话后会显示在这里",
    create: "创建一个",
    newButton: "新建会话",
    msgs: "条消息",
    newSession: "新会话",
    yesterday: "昨天",
    running: "运行中",
    copyId: "复制会话 ID",
    closeSession: "关闭会话",
    collapseSidebar: "收起侧栏",
    expandSidebar: "展开侧栏",
  },
  chatArea: {
    emptyTitle: "开始对话",
    emptyDesc: "在下方输入消息开始对话",
    chatInputPlaceholder: "输入消息... (Enter 发送, Shift+Enter 换行)",
    stop: "终止",
  },
  chatMessage: {
    you: "你",
    assistant: "助手",
    system: "系统",
    tool: "工具",
    compaction: "摘要",
    nudge: "轮询推进",
    context: "角色切换",
    copy: "复制",
    rate: "评价",
    copied: "已复制",
  },
  sidePanel: {
    providers: "服务商",
    models: "模型管理",
    noProviders: "未配置服务商。",
    noModels: "无可用模型。",
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
    systemLabel: "系统",
    systemDesc: "助手模式，附加系统工具。",
  },
  connectDialog: {
    title: "连接设置",
    mode: "模式",
    modeLocal: "本机",
    modeLocalHint: "通过 Tauri IPC 访问本机应用",
    modeRemote: "远程",
    modeRemoteHint: "通过网络访问内嵌服务",
    address: "服务地址",
    addressPlaceholder: "http://127.0.0.1:8787",
    token: "访问令牌（可选）",
    tokenHint: "后端白名单为空时无需填写",
    test: "测试连接",
    testing: "测试中...",
    reachable: "服务可达",
    unreachable: "无法连接",
    needUrl: "远程模式需要填写地址",
    save: "保存并切换",
    saving: "切换中...",
    cancel: "取消",
    switchFailed: "切换失败：{error}",
    lockedHint: "连接失败，请填写正确的服务地址后保存以继续。",
  },
  neuronListPanel: {
    title: "神经元",
    search: "搜索神经元...",
    kindAll: "全部",
    kindSystem: "系统",
    kindNormal: "普通",
    multiSelect: "多选",
    create: "新建",
    edit: "编辑",
    launch: "发起",
    launchHint: "以该神经元开启会话",
    loadMore: "加载更多 ↓",
    noMore: "没有更多了",
    empty: "暂无神经元。",
    loading: "加载中...",
  },
  neuronEditor: {
    systemType: "系统类型",
    systemTypeUnbound: "未绑定",
    bind: "绑定",
    rebind: "换绑",
    unbind: "取消绑定",
    bindPlaceholder: "如 session.my_spec",
    bindConfirmTitle: "绑定系统类型",
    bindConfirmBody: "将本神经元绑定为系统类型「{type}」？绑定后将出现行为管理控件。",
    unbindConfirmTitle: "取消绑定",
    unbindConfirmBody: "取消绑定后本神经元将变回普通神经元，行为管理控件将隐藏。确定继续？",
    behavior: "行为管理",
    saveBehavior: "保存行为",
    selection: "选型策略",
    tools: "工具策略",
    insertId: "契约手册",
    none: "无",
    fixed: "固定",
    neighborhood: "邻域",
    global: "全域",
    globalLimit: "全域数量",
    toolNone: "无",
    toolFromNeuron: "取神经元",
    toolAllowlist: "白名单",
    allowlistHint: "逗号分隔的工具 id",
    confirm: "确认",
    cancel: "取消",
    operationFailed: "操作失败",
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
  toolCall: {
    arguments: "参数",
  },
  thinking: {
    title: "思考过程",
  },
  toolResult: {
    executed: "命令执行",
    timedOut: "已超时",
    empty: "无输出",
    stdout: "标准输出",
    stderr: "标准错误",
  },
  terminal: {
    newTab: "新建终端",
    closeTab: "关闭终端",
    exited: "已退出",
    connecting: "正在连接终端服务…",
    disconnected: "终端连接已断开，正在重试…",
    empty: "暂无终端，点击 + 新建",
    exitCode: "进程已退出，退出码 {code}",
    initFailed: "终端初始化失败：{error}",
    spawnFailed: "新建失败：{error}",
    writeFailed: "写入失败：{error}",
  },
  logPanel: {
    verbosity: "日志级别",
    minLevel: "最小级别",
    target: "目标",
    keyword: "关键字",
    clear: "清空",
    file: "文件",
    empty: "没有符合当前筛选条件的日志。",
    targetPlaceholder: "neuron / gateway…",
    keywordPlaceholder: "phase / error_code…",
    initFailed: "日志初始化失败：{error}",
    setLevelFailed: "设置级别失败：{error}",
    clearFailed: "清空失败：{error}",
  },
  drawer: {
    sessions: "会话列表",
    info: "信息",
    panel: "面板",
  },
  topicPanel: {
    topics: "课题",
    all: "全部",
    filterActive: "进行中",
    filterDone: "已完成",
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
    openConversation: "打开对话",
    scopeItems: "范围项",
    addScopeItem: "添加项",
    scopeGoal: "目标",
    scopeContract: "完成条件",
    scopeAdd: "添加",
    scopeStatusPending: "待办",
    scopeStatusDone: "完成",
    scopeStatusBlocked: "等待用户",
    status: "状态",
    description: "描述",
    updated: "更新于",
    sessionId: "会话",
    name: "名称",
    topicStatus: {
      todo: "待办",
      in_progress: "进行中",
      paused: "已暂停",
      done: "已完成",
      cancelled: "已取消",
      waiting_user: "等待用户",
      wrapping_up: "收尾中",
    },
    createFailed: "创建失败：{error}",
    pauseFailed: "暂停失败：{error}",
    resumeFailed: "恢复失败：{error}",
    deleteFailed: "删除失败：{error}",
    addScopeFailed: "添加范围项失败：{error}",
    completeScopeFailed: "完成范围项失败：{error}",
    deleteScopeFailed: "删除范围项失败：{error}",
  },
  pollerPanel: {
    poller: "轮询器",
    status: "状态",
    running: "运行中",
    paused: "已暂停",
    tickCount: "Tick 数",
    taskCount: "任务数",
    interval: "间隔",
    parallelism: "并发推进",
    parallelismHint: "单次轮询同时推进的课题数（1–8），保存后立即生效。",
    save: "保存",
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
    delta: "增量",
    apply: "应用",
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
    posRight: "停靠右侧",
    posBottom: "停靠底部",
    create: "创建神经元",
    createTitle: "创建神经元",
    createOrphan: "孤立",
    createDownstream: "下游",
    createSource: "上游神经元",
    createSourcePlaceholder: "选择上游神经元...",
    createDescLabel: "描述",
    createDescPlaceholder: "神经元的简短描述",
    createContentLabel: "内容",
    createContentPlaceholder: "神经元承载的知识 / 内容（可选）",
    createDescRequired: "描述为必填项。",
    createSourceRequired: "请选择上游神经元。",
    createConfirm: "创建",
    creating: "创建中...",
    createDownstreamFromHere: "创建下游神经元",
    createToolIdsLabel: "工具",
    createToolIdsPlaceholder: "选择该神经元可用的工具（留空则无工具）",
    noToolsAvailable: "当前没有可用工具",
    id: "ID",
    copy: "复制",
    copied: "已复制",
    layoutLabel: "布局",
    layoutForce: "力导向",
    layoutLayered: "分层",
    setAsSeed: "设为画布核心",
  },
  toolPanel: {
    title: "工具",
    reload: "重新加载",
    editConfig: "编辑配置",
    loading: "加载中…",
    mcpSection: "MCP Servers",
    toolsSection: "工具",
    httpToolsSection: "HTTP Tools",
    commandToolsSection: "Command Tools",
    toolsCount: "{count} 个工具",
    noMcpServers: "暂无 MCP server，点右上角「编辑配置」添加",
    noTools: "暂无可用工具",
    status: {
      connecting: "连接中",
      connected: "已连接",
      failed: "失败",
      disabled: "已停用",
    },
    modalTitle: "工具配置",
    modalAria: "编辑工具配置",
    close: "关闭",
    loadingConfig: "加载配置中…",
    add: "添加",
    delete: "删除",
    emptyMcp: "暂无 MCP server",
    emptyHttp: "暂无 HTTP tool",
    emptyCommand: "暂无 command tool",
    name: "名称",
    transport: "传输方式",
    method: "方法",
    command: "命令",
    args: "参数（逗号分隔）",
    url: "地址",
    timeoutMs: "超时（毫秒）",
    desc: "描述",
    template: "模板（命令模板）",
    disabled: "停用",
    tag: "标签",
    transportHint: "stdio 需 command；http 需 url",
    httpUrlHint: "端点固定，{query} 由模型填充",
    commandHint: "命令经过安全护栏：denylist / 超时 / 并发",
    saveHint: "保存即生效：写回 JSON 并触发全量重装配",
    optional: "可选",
    descPlaceholder: "工具描述",
    cancel: "取消",
    save: "保存",
    saving: "保存中…",
    loadFailed: "加载配置失败",
    reassembleFailed: "重新装配失败：{error}",
    loadListFailed: "加载工具失败：{error}",
  },
  views: {
    sessions: "会话",
    providers: "服务商",
    models: "模型",
    topics: "课题",
    poller: "轮询器",
    tools: "工具",
    logs: "日志",
    terminal: "终端",
    chat: "对话",
    neurons: "神经元",
    toolEditor: "工具配置",
    neuronsList: "神经元",
    providersModels: "模型管理",
    providerManager: "模型管理",
    files: "文件",
    fileEditor: "文件编辑",
    git: "Git",
    gitDiff: "差异",
    search: "搜索",
    hookJudgements: "Hook 判定",
  },
  searchPanel: {
    placeholder: "搜索符号、函数、类型…",
    search: "搜索",
    indexing: "正在构建工作区索引…首次搜索耗时略长",
    noWorkspace: "尚未添加工作区",
    noQuery: "输入关键词开始搜索",
    empty: "未找到匹配块",
    openInEditor: "在编辑器中打开",
    loadFailed: "搜索失败：{error}",
    results: "{n} 个结果",
    blocks: "索引 {n} 块 · {ms} ms",
  },
  judgement: {
    status: {
      pending: "裁决中",
      ok: "成功",
      retriedOk: "重试成功",
      downgraded: "已降级",
    },
    hookType: "Hook",
    statusLabel: "状态",
    conversation: "会话",
    all: "全部",
    attempts: "尝试次数",
    durationMs: "耗时 (ms)",
    model: "模型",
    payload: "裁决输入",
    attemptsDetail: "尝试明细",
    rawResponse: "原始输出",
    decision: "决策",
    error: "错误",
    locate: "在会话中定位",
    empty: "暂无 Hook 判定记录",
    noMatch: "无匹配记录",
    running: "裁决中 · {hook}",
    refresh: "刷新",
    tooltip: {
      pending: "裁决执行中",
      ok: "一次成功",
      retriedOk: "首次失败，重试后成功",
      downgraded: "重试后仍失败，使用降级值兜底",
    },
    loadFailed: "加载失败：{error}",
  },
  hook: {
    completeScope: "范围完成",
    matchTopic: "课题匹配",
    reviseTopic: "课题修订",
    scoreFeedback: "评分反馈",
  },
  fileExplorer: {
    addWorkspace: "添加工作区",
    addWorkspaceHint: "选择项目目录开始管理文件",
    selectWorkspace: "选择工作区",
    addWorkspaceInputPlaceholder: "输入目录绝对路径",
    addWorkspaceInputConfirm: "添加",
    addWorkspaceBrowse: "浏览…",
    workspaceActions: "工作区操作",
    editIgnore: "编辑过滤规则",
    deleteWorkspace: "删除工作区",
    deleteWorkspaceConfirm: "仅移除配置，不删除磁盘文件。确定移除该工作区？",
    ignoreTitle: "过滤规则",
    ignoreHint: "每行一条（glob 或名称，如 node_modules、*.log）；默认过滤 .git / node_modules / target / dist / .pulsar",
    ignoreSave: "保存",
    ignoreCancel: "取消",
    refresh: "刷新",
    newFile: "新建文件",
    newFolder: "新建文件夹",
    rename: "重命名",
    delete: "删除",
    move: "移动…",
    moveTargetHint: "移动到：选择目标目录",
    moveConfirm: "移动",
    copyPath: "复制路径",
    open: "打开",
    empty: "此目录为空",
    noWorkspace: "尚未添加工作区",
    loading: "加载中…",
    loadFailed: "加载失败",
    invalidName: "名称不合法",
    operationFailed: "操作失败：{error}",
    copied: "已复制",
  },
  fileEditor: {
    save: "保存",
    saving: "保存中…",
    saved: "已保存",
    saveFailed: "保存失败：{error}",
    closeUnsavedTitle: "关闭未保存文件",
    closeUnsavedBody: "文件有未保存修改，确定关闭？",
    discard: "不保存关闭",
    conflictTitle: "文件已被外部修改",
    conflictBody: "磁盘上的文件自打开后已被修改。覆盖将丢失外部改动，仍要继续？",
    overwrite: "覆盖",
    cancel: "取消",
    fileMissing: "文件不存在或已被移动",
    workspaceMismatch: "工作区已切换；切回该文件所在工作区后再编辑",
    binaryRejected: "二进制文件无法打开",
    loadFailed: "打开失败：{error}",
    loading: "加载中…",
    loadedLines: "已加载 {loaded}/{total} 行",
    loadMore: "滚动到底部加载更多",
  },
  git: {
    repo: "仓库",
    repoPlaceholder: "选择仓库",
    noRepos: "未发现 Git 仓库",
    branch: "分支",
    summary: "{staged} 暂存 · {changes} 更改",
    groupStaged: "暂存",
    groupChanges: "更改",
    groupConflicted: "冲突",
    groupLog: "提交记录",
    logEmpty: "无提交记录",
    logLoading: "加载中…",
    logMore: "加载更多",
    openCommitDiff: "在主区域打开",
    status: {
      "??": "未跟踪",
      M: "已修改",
      A: "已新增",
      D: "已删除",
      R: "已重命名",
      U: "冲突",
      MM: "已暂存修改，工作区又有改动",
      AM: "已暂存新增，工作区又有改动",
      AD: "已暂存新增，工作区已删除",
      DD: "双方已删除",
      UU: "双方修改冲突",
    },
    states: {
      M: "已修改",
      A: "已新增",
      D: "已删除",
      R: "已重命名",
      C: "已复制",
      U: "冲突",
      "?": "未跟踪",
      T: "类型变更",
      "!": "已忽略",
      " ": "未修改",
    },
    statusTemplate: "暂存：{x} / 更改：{y}",
    stage: "暂存",
    unstage: "取消暂存",
    stageAll: "全部暂存",
    unstageAll: "全部取消暂存",
    filesCount: "{n} 个文件",
    commitPlaceholder: "提交信息…",
    commit: "提交",
    committing: "提交中…",
    nothingToCommit: "没有可提交的更改",
    branches: "分支",
    stash: "Stash",
    stashCreate: "Stash…",
    stashApply: "应用",
    stashDrop: "丢弃",
    notRepo: "当前工作区不是 Git 仓库",
    clean: "工作区干净",
    pull: "拉到最新",
    push: "推送到远端",
    discard: "丢弃更改",
    editIgnore: "编辑仓库过滤规则",
    refresh: "刷新",
    loadFailed: "加载失败：{error}",
    operationFailed: "操作失败：{error}",
    checkoutFailed: "切换分支失败：{error}",
    commitConfirmTitle: "提交更改",
    commitConfirmBody: "将 {n} 个暂存文件提交？",
    pushConfirmTitle: "推送",
    pushConfirmBody: "推送 {branch}（领先 {ahead}）？",
    pullConfirmTitle: "拉取",
    pullConfirmBody: "拉取最新并合并到 {branch}？本地改动可能冲突。",
    discardConfirmTitle: "丢弃更改",
    discardConfirmBody: "将永久丢弃 {n} 个文件中的改动，继续？",
    checkoutConfirmBody: "切换到 {target}？未提交的更改可能被覆盖。",
    stashApplyConfirmTitle: "应用 Stash",
    stashApplyConfirmBody: "应用该 stash？可能产生冲突。",
    stashDropConfirmTitle: "丢弃 Stash",
    stashDropConfirmBody: "丢弃该 stash？此操作无法撤销。",
    confirmDiscard: "确认丢弃",
    dangerousWrites: "允许危险写操作（reset --hard / checkout 覆盖未提交改动）",
    rangeStaged: "暂存（vs HEAD）",
    rangeUnstaged: "更改（vs 暂存）",
    rangeBoth: "全部更改",
    prevHunk: "上一处",
    nextHunk: "下一处",
    hunkCount: "{current}/{total}",
    acceptOurs: "接受当前",
    acceptTheirs: "接受传入",
    acceptBoth: "接受两者",
    blame: "Blame",
    blameLoading: "分析行归属中…",
    diffEmpty: "无差异",
    untrackedHint: "未跟踪文件",
    binaryDiff: "二进制文件，无法显示差异",
    missingFile: "文件不存在",
  },
  providersModelsPanel: {
    create: "新增服务商",
    expand: "展开",
    collapse: "收起",
    delete: "删除",
    deleteConfirm: "删除该服务商？内置服务商禁用后将从列表隐藏。",
    deleteGo: "删除",
    cancel: "取消",
  },
  providerManager: {
    modalTitle: "模型管理",
    loading: "加载配置中…",
    providers: "服务商",
    addProvider: "新增服务商",
    untitled: "未命名",
    disabled: "已禁用",
    builtin: "内置",
    noProviders: "暂无服务商",
    providerFields: "服务商信息",
    id: "ID",
    displayName: "显示名称",
    kind: "类型",
    apiBase: "API 地址",
    authEnv: "认证环境变量",
    apiKey: "API Key",
    apiKeyMasked: "已配置（掩码显示）；输入新值将替换",
    apiKeyPlaceholder: "sk-... （留空则不设置）",
    enabled: "启用",
    models: "模型",
    addModel: "添加模型",
    noModels: "未配置模型",
    contextWindow: "上下文（tokens）",
    maxOutput: "最大输出",
    priceIn: "输入价格（$/M）",
    priceOut: "输出价格（$/M）",
    deleteModel: "删除模型",
    deleteProvider: "删除服务商",
    builtinDeleteHint: "内置服务商无法物理删除。禁用后将从列表隐藏，可随时在此重新启用。",
    disableProvider: "禁用该服务商",
    customDeleteHint: "删除自定义服务商将彻底从 config.json 移除。",
    selectProvider: "在左侧选择一个服务商进行编辑。",
    defaultsHint: "默认模型：{provider} / {model}",
    noDefaults: "未配置默认模型。",
    cancel: "取消",
    save: "保存",
    saving: "保存中…",
  },
};

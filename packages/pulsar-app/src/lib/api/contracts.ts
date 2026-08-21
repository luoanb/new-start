/**
 * 命令契约表：后端 RPC 命令的唯一真源（类型安全收束）。
 *
 * 业务层禁止再裸写 `api.invoke("cmd", params)` 魔法字符串，一律
 * `api.call(c.someCommand, params)` —— 参数与返回类型由契约编译期校验，
 * 后端改命令时只需改这一处。字段名保持与后端 Tauri command 参数一致
 * （snake_case 为后端参数名，如 `max_depth` / `op_id`）。
 */
import type {
  ChatModelSelection,
  ChatResponse,
  ConflictTake,
  Connection,
  Conversation,
  FsEntry,
  FsInfo,
  FsReadResult,
  FsWriteResult,
  GitBlameLine,
  GitBranchItem,
  GitCommitInfo,
  GitDiff,
  GitFileDiff,
  GitRepo,
  GitResetMode,
  GitShowFile,
  GitStashAction,
  GitStashEntry,
  GitStatusView,
  LogEntry,
  McpServerStatus,
  Message,
  ModelInfo,
  Neuron,
  NeuronPage,
  NeuronSubgraph,
  PollerStatus,
  ProviderConfigView,
  ProviderInfo,
  RunningSession,
  RuntimeStatus,
  SamplingParams,
  SkillInfo,
  ThinkingConfig,
  ToolConfigView,
  ToolInfo,
  Topic,
  WorkspaceView,
} from "$lib/types";
import type { TerminalSessionInfo } from "$lib/terminal/transport";

/** 命令契约：P = 参数类型，R = 返回类型（均为编译期 phantom，运行时只携带命令名）。 */
export interface Contract<P, R> {
  /** 后端命令名（Tauri IPC 命令名 / POST /api/rpc 的 cmd 字段）。 */
  readonly cmd: string;
  readonly __p?: P;
  readonly __r?: R;
}

/** 声明命令契约（零运行时开销）。 */
export function def<P, R>(cmd: string): Contract<P, R> {
  return { cmd } as unknown as Contract<P, R>;
}

/** 神经元列表 kind 过滤（与 NeuronListPanel 工具栏一致）。 */
export type NeuronKindFilter = "all" | "system" | "normal";

/**
 * 全部后端命令契约，按领域分组。未列出的命令 = 前端当前未调用。
 */
export const c = {
  // ── 会话 / 消息 ──
  listRunningSessions: def<undefined, RunningSession[]>("list_running_sessions"),
  history: def<{ conversationId: string }, Message[]>("history"),
  listConversations: def<undefined, Conversation[]>("list_conversations"),
  createConversation: def<{ mode: string }, string>("create_conversation"),
  closeSession: def<{ sessionId: string }, string>("close_session"),
  sendChatMessage: def<
    {
      message: string;
      providerId: string;
      modelId: string;
      conversationId: string;
      params?: SamplingParams;
      thinking?: ThinkingConfig;
    },
    ChatResponse
  >("send_chat_message"),
  converseSession: def<
    { sessionId: string; input: string; providerId: string; modelId: string },
    ChatResponse
  >("converse_session"),
  openSession: def<{ specNeuronId: string; mode: string }, Conversation>("open_session"),
  setSessionModel: def<
    { conversationId: string; selection: ChatModelSelection },
    void
  >("set_session_model"),
  clearConversation: def<{ conversationId: string | null }, string>("clear_conversation"),
  scoreFeedback: def<
    { conversationId: string; messageIndex: number; score: number },
    void
  >("score_feedback"),

  // ── 主题（Topic）──
  listTopics: def<undefined, Topic[]>("list_topics"),
  getTopic: def<{ id: string }, Topic>("get_topic"),
  createTopic: def<{ name: string; description: string }, Topic>("create_topic"),
  deleteTopic: def<{ id: string }, boolean>("delete_topic"),
  pauseTopic: def<{ id: string }, Topic>("pause_topic"),
  resumeTopic: def<{ id: string }, Topic>("resume_topic"),
  addTopicScopeItem: def<
    { topicId: string; goal: string; doneContract: string },
    Topic
  >("add_topic_scope_item"),
  completeTopicScopeItem: def<{ topicId: string; itemId: string }, Topic>(
    "complete_topic_scope_item",
  ),
  deleteTopicScopeItem: def<{ topicId: string; itemId: string }, Topic>(
    "delete_topic_scope_item",
  ),

  // ── Poller ──
  pollStatus: def<undefined, PollerStatus>("poll_status"),
  pollPause: def<undefined, void>("poll_pause"),
  pollResume: def<undefined, void>("poll_resume"),
  pollTrigger: def<undefined, void>("poll_trigger"),
  pollSetParallelism: def<{ n: number }, number>("poll_set_parallelism"),

  // ── 工作区 ──
  listWorkspaces: def<undefined, WorkspaceView>("list_workspaces"),
  addWorkspace: def<{ root: string }, WorkspaceView>("add_workspace"),
  removeWorkspace: def<{ id: string }, WorkspaceView>("remove_workspace"),
  setActiveWorkspace: def<{ id: string }, WorkspaceView>("set_active_workspace"),
  updateWorkspaceIgnore: def<{ id: string; ignore: string[] }, WorkspaceView>(
    "update_workspace_ignore",
  ),

  // ── 服务商 / 模型 ──
  listProviders: def<undefined, ProviderInfo[]>("list_providers"),
  listModels: def<undefined, ModelInfo[]>("list_models"),
  getProviderConfig: def<undefined, ProviderConfigView>("get_provider_config"),
  saveProviderConfig: def<{ view: ProviderConfigView }, ProviderConfigView>(
    "save_provider_config",
  ),

  // ── 工具 / 技能 ──
  listSkills: def<undefined, SkillInfo[]>("list_skills"),
  listTools: def<undefined, ToolInfo[]>("list_tools"),
  listMcpServers: def<undefined, McpServerStatus[]>("list_mcp_servers"),
  getToolConfig: def<undefined, ToolConfigView>("get_tool_config"),
  saveToolConfig: def<{ view: ToolConfigView }, ToolConfigView>("save_tool_config"),
  reassembleTools: def<undefined, void>("reassemble_tools"),
  listInsertCatalog: def<undefined, { id: string; hint: string }[]>("list_insert_catalog"),

  // ── 神经元 ──
  listNeurons: def<undefined, Neuron[]>("list_neurons"),
  createNeuronPlain: def<
    { desc: string; content: string; linkTo: string | null; toolIds: string[] },
    Neuron
  >("create_neuron_plain"),
  listNeuronsPage: def<
    {
      page: number;
      pageSize: number;
      search: string | null;
      kind: NeuronKindFilter;
    },
    NeuronPage
  >("list_neurons_page"),
  getNeuron: def<{ id: string }, Neuron>("get_neuron"),
  updateNeuron: def<
    { id: string; desc?: string | null; content?: string | null; toolIds?: string[] },
    Neuron
  >("update_neuron"),
  getConnections: def<{ id: string }, Connection[]>("get_connections"),
  getNetwork: def<{ id: string; max_depth: number }, NeuronSubgraph>("get_network"),
  setNeuronSystemType: def<{ id: string; systemType: string | null }, Neuron>(
    "set_neuron_system_type",
  ),
  updateNeuronBehavior: def<{ id: string; behavior: unknown }, Neuron>(
    "update_neuron_behavior",
  ),
  adjustNeuronWeight: def<{ id: string; delta: number }, Neuron>("adjust_neuron_weight"),
  adjustEdgeWeight: def<{ source: string; target: string; delta: number }, Connection>(
    "adjust_edge_weight",
  ),

  // ── Git ──
  gitRepos: def<undefined, GitRepo[]>("git_repos"),
  gitSetActiveRepo: def<{ repoId: string }, void>("git_set_active_repo"),
  gitStatus: def<{ repoId: string }, GitStatusView>("git_status"),
  gitDiff: def<{ repoId: string; path: string; cached: boolean }, GitDiff>("git_diff"),
  gitLog: def<undefined, GitCommitInfo[]>("git_log"),
  gitShowFiles: def<{ hash: string }, GitShowFile[]>("git_show_files"),
  gitShowDiff: def<{ hash: string; path: string }, GitFileDiff>("git_show_diff"),
  gitBranches: def<undefined, GitBranchItem[]>("git_branches"),
  gitBlame: def<{ repoId: string; path: string }, GitBlameLine[]>("git_blame"),
  gitStashList: def<undefined, GitStashEntry[]>("git_stash_list"),
  gitGetConfirmConfig: def<undefined, { dangerous_writes: boolean }>("git_get_confirm_config"),
  gitAdd: def<{ paths: string[]; all: boolean }, void>("git_add"),
  gitUnstage: def<{ paths: string[] }, void>("git_unstage"),
  gitRestore: def<{ paths: string[] }, void>("git_restore"),
  gitCommit: def<{ message: string }, void>("git_commit"),
  gitReset: def<{ mode: GitResetMode; target?: string }, void>("git_reset"),
  gitCheckout: def<{ target: string }, void>("git_checkout"),
  gitStash: def<{ action: GitStashAction; message?: string }, void>("git_stash"),
  gitPush: def<{ remote?: string; branch?: string }, void>("git_push"),
  gitPull: def<undefined, void>("git_pull"),
  gitResolveConflict: def<{ repoId?: string; path: string; take: ConflictTake }, void>(
    "git_resolve_conflict",
  ),
  gitConfirm: def<{ opId: string; approved: boolean }, void>("git_confirm"),
  gitSetDangerousWrites: def<{ enabled: boolean }, void>("git_set_dangerous_writes"),

  // ── 文件系统 ──
  fsList: def<{ path?: string }, FsEntry[]>("fs_list"),
  fsSuggestAbs: def<{ path: string }, FsEntry[]>("fs_suggest_abs"),
  fsRead: def<{ path: string; offset?: number; limit?: number }, FsReadResult>("fs_read"),
  fsWrite: def<
    { path: string; content: string; base_mtime?: number | null },
    FsWriteResult
  >("fs_write"),
  fsInfo: def<{ path: string }, FsInfo>("fs_info"),
  fsCreateDir: def<{ path: string }, void>("fs_create_dir"),
  fsDelete: def<{ paths: string[] }, void>("fs_delete"),
  fsRename: def<{ from: string; to: string }, void>("fs_rename"),
  fsMove: def<{ from: string; to: string }, void>("fs_move"),
  getHomeDir: def<undefined, string>("get_home_dir"),

  // ── 日志 ──
  logsSnapshot: def<undefined, LogEntry[]>("logs_snapshot"),
  logsGetLevel: def<undefined, string>("logs_get_level"),
  logsSetLevel: def<{ level: string }, string>("logs_set_level"),
  logsClearBuffer: def<undefined, void>("logs_clear_buffer"),
  logsDir: def<undefined, string | null>("logs_dir"),

  // ── 终端 ──
  terminalSpawn: def<
    { cwd?: string; shell?: string; cols?: number; rows?: number },
    { sessionId: string }
  >("terminal_spawn"),
  terminalWrite: def<{ sessionId: string; data: string }, void>("terminal_write"),
  terminalResize: def<{ sessionId: string; cols: number; rows: number }, void>(
    "terminal_resize",
  ),
  terminalKill: def<{ sessionId: string }, void>("terminal_kill"),
  terminalList: def<undefined, TerminalSessionInfo[]>("terminal_list"),

  // ── 系统 ──
  status: def<undefined, RuntimeStatus>("status"),
  debugStoragePath: def<undefined, string>("debug_storage_path"),
} as const;

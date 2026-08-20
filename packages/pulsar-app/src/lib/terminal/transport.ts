/**
 * 终端传输抽象：统一桌面（Tauri IPC）与浏览器（WebSocket PTY 网关）两路通道。
 *
 * 协议对照（见 spec `2026-08-20_11-30_terminal-browser-ws.md`）：
 * - 桌面：`terminal_spawn / write / resize / kill / list` 命令 + `app://terminal-output / exit` 事件。
 * - 浏览器：`spawn / write / resize / kill / list` JSON 帧（c→s）；`spawned / output / exit / list / error` 帧（s→c）。
 *   `write` 与 `output` 的 data 均为 base64 编码的字节串。
 */
import { api } from "$lib/api";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** 后端 `terminal_list` 返回的会话元信息（IPC 与 WS 的 list 帧结构一致）。 */
export type TerminalSessionInfo = {
  sessionId: string;
  shell: string;
  cwd?: string | null;
  exitCode?: number | null;
};

/** 传输连接状态（IPC 恒为 connected；WS 有断线重连）。 */
export type TerminalConnStatus = "connecting" | "connected" | "disconnected";

/** 终端传输接口：命令 + 事件订阅 + 连接状态。 */
export type TerminalTransport = {
  spawn(opts?: { cwd?: string; shell?: string; cols?: number; rows?: number }): Promise<string>;
  write(sessionId: string, data: string): Promise<void>;
  resize(sessionId: string, cols: number, rows: number): Promise<void>;
  kill(sessionId: string): Promise<void>;
  list(): Promise<TerminalSessionInfo[]>;
  /** 会话输出回调；data 为解码后的字节流。返回退订函数。 */
  onOutput(cb: (sessionId: string, data: Uint8Array) => void): () => void;
  /** 会话退出回调；返回退订函数。 */
  onExit(cb: (sessionId: string, exitCode: number) => void): () => void;
  status(): TerminalConnStatus;
  /** 连接状态变化回调；返回退订函数。 */
  onStatusChange(cb: (status: TerminalConnStatus) => void): () => void;
  /** 主动释放连接资源（组件销毁时调用；IPC 传输为 no-op）。 */
  dispose?(): void;
};

/** Tauri IPC 传输：事件走 `app://terminal-output / exit`，命令走 `api.invoke`。 */
export function ipcTransport(): TerminalTransport {
  const outputCbs = new Set<(sessionId: string, data: Uint8Array) => void>();
  const exitCbs = new Set<(sessionId: string, exitCode: number) => void>();
  let unlisteners: UnlistenFn[] = [];

  const started = Promise.all([
    listen<{ sessionId: string; data: number[] }>("app://terminal-output", (e) => {
      const bytes = new Uint8Array(e.payload.data);
      outputCbs.forEach((cb) => cb(e.payload.sessionId, bytes));
    }),
    listen<{ sessionId: string; exitCode: number }>("app://terminal-exit", (e) => {
      exitCbs.forEach((cb) => cb(e.payload.sessionId, e.payload.exitCode));
    }),
  ]).then((fns) => {
    unlisteners = fns;
  }).catch(() => {
    // 事件通道不可用（如 Tauri 事件系统初始化异常）：命令仍可用，事件静默丢失。
  });

  return {
    async spawn(opts) {
      await started;
      const { sessionId } = await api.invoke<{ sessionId: string }>("terminal_spawn", opts ?? {});
      return sessionId;
    },
    async write(sessionId, data) {
      await api.invoke("terminal_write", { sessionId, data });
    },
    async resize(sessionId, cols, rows) {
      await api.invoke("terminal_resize", { sessionId, cols, rows });
    },
    async kill(sessionId) {
      await api.invoke("terminal_kill", { sessionId });
    },
    async list() {
      await started;
      return api.invoke<TerminalSessionInfo[]>("terminal_list");
    },
    onOutput(cb) {
      outputCbs.add(cb);
      return () => outputCbs.delete(cb);
    },
    onExit(cb) {
      exitCbs.add(cb);
      return () => exitCbs.delete(cb);
    },
    status: () => "connected" as const,
    onStatusChange: () => () => {},
  };
}

/** base64 → 字节串（浏览器解码，兼容任意二进制输出）。 */
function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

/** 字符串 → base64（UTF-8 编码，与后端 `sh`/`cmd` 写入的字节一致）。 */
function bytesToBase64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let bin = "";
  // 分块避免超长字符串导致的调用栈压力。
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(bin);
}

/**
 * WebSocket 传输：直连 Tauri 进程内嵌的 PTY 网关，自动重连。
 *
 * 响应帧与请求按序匹配（网关单连接串行处理请求）；`output/exit` 为事件帧，
 * 与请求响应交错出现，单独分发。
 */
export function wsTransport(url: string): TerminalTransport {
  let ws: WebSocket | null = null;
  let closed = false; // 主动 close() 后停止重连
  let status: TerminalConnStatus = "connecting";
  let reconnectTimer: ReturnType<typeof setTimeout> | undefined;

  const outputCbs = new Set<(sessionId: string, data: Uint8Array) => void>();
  const exitCbs = new Set<(sessionId: string, exitCode: number) => void>();
  const statusCbs = new Set<(status: TerminalConnStatus) => void>();
  // 待匹配的请求（响应顺序与请求顺序一致，FIFO 匹配）。
  const pending: { resolve: (v: unknown) => void; reject: (e: Error) => void }[] = [];

  // 连接就绪等待：组件初始化即发出的 list/spawn 请求在连接建立前排队等待，
  // 而不是立刻报 "not connected"（连接是异步的，首次打开面板必然存在竞态）。
  // 订阅状态变化实现：connected 放行、dispose 中止；服务器不可达时跟随重连循环
  // 持续等待（面板显示 connecting 横幅），不产生忙等。
  const isOpen = () => !!ws && ws.readyState === WebSocket.OPEN;
  const waitConnected = (): Promise<void> =>
    new Promise<void>((resolve, reject) => {
      if (closed) return reject(new Error("terminal ws: disposed"));
      if (isOpen()) return resolve();
      const onStatus = (s: TerminalConnStatus) => {
        if (s === "connected") {
          cleanup();
          resolve();
        } else if (closed) {
          cleanup();
          reject(new Error("terminal ws: disposed"));
        }
      };
      const cleanup = () => statusCbs.delete(onStatus);
      statusCbs.add(onStatus);
    });

  const setStatus = (s: TerminalConnStatus) => {
    if (status === s) return;
    status = s;
    statusCbs.forEach((cb) => cb(s));
  };

  const connect = () => {
    if (closed) return;
    setStatus("connecting");
    try {
      ws = new WebSocket(url);
    } catch {
      setStatus("disconnected");
      scheduleReconnect();
      return;
    }
    ws.onopen = () => setStatus("connected");
    ws.onerror = () => {
      ws?.close();
    };
    ws.onclose = () => {
      ws = null;
      // 连接中断：挂起的请求全部失败。
      const error = new Error("terminal ws: connection closed");
      while (pending.length) pending.shift()!.reject(error);
      if (!closed) {
        setStatus("disconnected");
        scheduleReconnect();
      }
    };
    ws.onmessage = (ev) => {
      let frame: {
        topic?: string;
        type: string;
        sessionId?: string;
        data?: string;
        exitCode?: number;
        sessions?: TerminalSessionInfo[];
        message?: string;
      };
      try {
        frame = JSON.parse(String(ev.data));
      } catch {
        return;
      }
      // 仅处理终端业务帧（topic: "terminal"）；连接级 error 帧（topic: "_error"）等
      // 其他业务帧不属于本传输，忽略。
      if (frame.topic !== "terminal") return;
      switch (frame.type) {
        case "output":
          if (frame.sessionId && frame.data != null) {
            const bytes = base64ToBytes(frame.data);
            outputCbs.forEach((cb) => cb(frame.sessionId!, bytes));
          }
          return;
        case "exit":
          if (frame.sessionId && frame.exitCode != null) {
            exitCbs.forEach((cb) => cb(frame.sessionId!, frame.exitCode!));
          }
          return;
        case "error":
          if (pending.length) {
            pending.shift()!.reject(new Error(frame.message ?? "terminal ws: unknown error"));
          } else {
            console.warn("[terminal ws] server error:", frame.message);
          }
          return;
        default:
          break;
      }
      // 响应帧：与最旧的待匹配请求配对。
      const waiter = pending.shift();
      if (waiter) waiter.resolve(frame);
      else console.warn("[terminal ws] unmatched response frame:", frame);
    };
  };

  const scheduleReconnect = () => {
    if (closed || reconnectTimer) return;
    reconnectTimer = setTimeout(() => {
      reconnectTimer = undefined;
      connect();
    }, 2000);
  };

  const send = async <T>(frame: Record<string, unknown>): Promise<T> => {
    // 连接尚未建立（首次打开面板 / 断线重连期间）：等待连接就绪，不立即报错。
    await waitConnected();
    if (closed) throw new Error("terminal ws: disposed");
    return new Promise<T>((resolve, reject) => {
      pending.push({ resolve: resolve as (v: unknown) => void, reject });
      // WS 为公共服务：请求帧统一带 topic 信封，路由到终端业务。
      ws!.send(JSON.stringify({ topic: "terminal", ...frame }));
    });
  };

  connect();

  return {
    async spawn(opts) {
      const frame = await send<{ type: string; sessionId: string }>({
        type: "spawn",
        ...(opts?.cwd != null && { cwd: opts.cwd }),
        ...(opts?.shell != null && { shell: opts.shell }),
        ...(opts?.cols != null && { cols: opts.cols }),
        ...(opts?.rows != null && { rows: opts.rows }),
      });
      return frame.sessionId;
    },
    async write(sessionId, data) {
      await send({ type: "write", sessionId, data: bytesToBase64(data) });
    },
    async resize(sessionId, cols, rows) {
      await send({ type: "resize", sessionId, cols, rows });
    },
    async kill(sessionId) {
      await send({ type: "kill", sessionId });
    },
    async list() {
      const frame = await send<{ type: string; sessions: TerminalSessionInfo[] }>({
        type: "list",
      });
      return frame.sessions ?? [];
    },
    onOutput(cb) {
      outputCbs.add(cb);
      return () => outputCbs.delete(cb);
    },
    onExit(cb) {
      exitCbs.add(cb);
      return () => exitCbs.delete(cb);
    },
    status: () => status,
    onStatusChange(cb) {
      statusCbs.add(cb);
      return () => statusCbs.delete(cb);
    },
    /** 主动关闭连接（组件销毁时调用）。 */
    dispose() {
      closed = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      ws?.close();
      ws = null;
      setStatus("disconnected"); // 唤醒 waitConnected 的等待者并使其失败
      while (pending.length) pending.shift()!.reject(new Error("terminal ws: disposed"));
    },
  };
}

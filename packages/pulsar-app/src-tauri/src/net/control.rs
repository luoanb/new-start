//! 内嵌 server 运行时控制器（桌面端）。
//!
//! 职责分层（对齐 Linux 服务管理）：
//! - **配置**（`config.json` `server` 节）：`enabled`（是否随应用启动自动开启，≈ systemd enable）、
//!   `lan`、`port`、`tokens` —— 只经 `set_config` 读写，不影响运行态。
//! - **运行态**：`start` / `stop`（≈ systemctl start/stop）—— 只改运行态，不改配置。
//! - **查询**：`info` 汇总运行态 + 配置 + 网卡地址。
//!
//! 绑定地址由配置 `lan` 派生（`lan` → `0.0.0.0`，否则 `127.0.0.1`；env `PULSAR_HOST` 优先）；
//! 非 loopback 且白名单为空时 `start` 自动生成访问令牌，避免局域网裸奔。

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use serde::Deserialize;
use tokio::sync::{broadcast, oneshot};

use crate::application::gateway::Gateway;
use crate::core::{StateChange, StateEmitter};
use crate::infra::config::{
    server_env_overrides, ConfigStore, ServerSection, DEFAULT_SERVER_PORT,
};
use crate::terminal::{events::TerminalEventHub, manager::TerminalManager};

use super::{
    bind_listener, is_lan_host, lan_addresses, serve_with_shutdown, NetState, ServerConfig,
    ServerInfo,
};

/// 仅本机监听地址（`lan=false`）。
const LOOPBACK_HOST: &str = "127.0.0.1";
/// 全网卡监听地址（`lan=true`）。
const LAN_HOST: &str = "0.0.0.0";
/// 优雅退出等待窗口：超时则强制取消 serve 任务（仍持有长连接时，如 SSE）。
const GRACEFUL_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1000);

/// 配置局部更新：仅写入提供的字段（`None` = 保持不变）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServerConfigPatch {
    pub enabled: Option<bool>,
    pub lan: Option<bool>,
    pub port: Option<u16>,
    pub tokens: Option<Vec<String>>,
}

/// 服务控制动作（≈ systemctl start/stop/restart）。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerAction {
    Start,
    Stop,
    Restart,
}

/// 生成随机访问令牌（16 字节 → 32 位 hex）。
fn generate_token() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| format!("generate token failed: {error}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// 运行中的内嵌 server 句柄：触发优雅退出的通道 + serve 任务句柄 + 实际监听地址。
///
/// `handle` 用于停止时**等待监听端口真正释放**后返回；否则紧接的 rebind 会因
/// 旧 listener 尚未销毁而 `EADDRINUSE`。
struct RunningServer {
    shutdown: oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
    host: String,
    port: u16,
}

/// 内嵌 server 运行时控制器（Tauri managed state）。
pub struct ServerController {
    storage_root: PathBuf,
    gateway: Gateway,
    state_emit: StateEmitter,
    events_tx: broadcast::Sender<StateChange>,
    terminal: Arc<TerminalManager>,
    terminal_hub: TerminalEventHub,
    inner: Mutex<Option<RunningServer>>,
}

impl ServerController {
    pub fn new(
        storage_root: PathBuf,
        gateway: Gateway,
        state_emit: StateEmitter,
        events_tx: broadcast::Sender<StateChange>,
        terminal: Arc<TerminalManager>,
        terminal_hub: TerminalEventHub,
    ) -> Self {
        Self {
            storage_root,
            gateway,
            state_emit,
            events_tx,
            terminal,
            terminal_hub,
            inner: Mutex::new(None),
        }
    }

    /// 配置 `enabled`：是否随应用启动自动开启（启动期据此决定是否自动拉起）。
    pub fn auto_start(&self) -> bool {
        self.read_section()
            .and_then(|section| section.enabled)
            .unwrap_or(false)
    }

    pub fn is_running(&self) -> bool {
        self.inner.lock().map(|guard| guard.is_some()).unwrap_or(false)
    }

    /// 汇总运行态 + 配置 + 网卡地址。`enabled` = 是否在跑；`auto_start`/`lan` = 配置值。
    pub fn info(&self) -> ServerInfo {
        let cfg = self.resolve_config();
        let section = self.read_section();
        let running = self
            .inner
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|r| (r.host.clone(), r.port)));
        let (host, port) = running.unwrap_or((cfg.host, cfg.port));
        ServerInfo {
            version: env!("CARGO_PKG_VERSION"),
            running: self.is_running(),
            enabled: section.as_ref().and_then(|s| s.enabled).unwrap_or(false),
            lan: section.as_ref().and_then(|s| s.lan).unwrap_or(false),
            lan_addresses: lan_addresses(),
            host,
            port,
            static_enabled: cfg!(feature = "embed-static"),
            auth_required: !cfg.tokens.is_empty(),
            token: cfg.tokens.first().cloned(),
        }
    }

    /// 写配置（只落盘，**不影响运行态**）：新配置在下次 `start` 时生效。
    pub fn set_config(&self, patch: ServerConfigPatch) -> Result<ServerInfo, String> {
        self.persist(|section| {
            if let Some(enabled) = patch.enabled {
                section.enabled = Some(enabled);
            }
            if let Some(lan) = patch.lan {
                section.lan = Some(lan);
            }
            if let Some(port) = patch.port {
                section.port = Some(port);
            }
            if let Some(tokens) = patch.tokens {
                section.tokens = Some(tokens);
            }
        })?;
        Ok(self.info())
    }

    /// 服务控制（≈ systemctl start/stop/restart）：只改运行态，不改配置。
    pub async fn control(&self, action: ServerAction) -> Result<ServerInfo, String> {
        match action {
            ServerAction::Start => self.start().await,
            ServerAction::Stop => self.stop().await,
            ServerAction::Restart => {
                // 先停并等端口释放，再按当前配置重新绑定（配置改动即在此生效）。
                self.shutdown_running().await;
                self.start().await
            }
        }
    }

    /// 启动（幂等）：已运行直接返回。bind 失败返回 `Err`，不残留句柄。
    async fn start(&self) -> Result<ServerInfo, String> {
        if self.is_running() {
            return Ok(self.info());
        }
        let mut cfg = self.resolve_config();
        // 监听非 loopback 时强制要求 token：白名单为空则自动生成并持久化，
        // 否则 auth 中间件（白名单为空即放行）会让同网段任何人无认证访问后端。
        if is_lan_host(&cfg.host) && cfg.tokens.is_empty() {
            let token = generate_token()?;
            let persisted = token.clone();
            self.persist(move |section| section.tokens = Some(vec![persisted]))?;
            cfg.tokens = vec![token];
        }
        let listener = bind_listener(&cfg.host, cfg.port).await?;
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let state = NetState {
            gateway: self.gateway.clone(),
            state_emit: self.state_emit.clone(),
            events_tx: self.events_tx.clone(),
            tokens: cfg.tokens.clone(),
            terminal: self.terminal.clone(),
            terminal_hub: self.terminal_hub.clone(),
            host: cfg.host.clone(),
            port: cfg.port,
        };
        tracing::info!(
            addr = %format!("{}:{}", cfg.host, cfg.port),
            token_count = cfg.tokens.len(),
            "network server listening (remote mode)"
        );
        let handle = tokio::spawn(async move {
            if let Err(error) = serve_with_shutdown(listener, state, async move {
                let _ = shutdown_rx.await;
            })
            .await
            {
                tracing::error!(error = %error, "network server exited unexpectedly");
            }
        });
        // 作用域内持锁写入，务必在调用 info() 前释放，否则与 info() 的加锁互死锁。
        {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| "server controller lock poisoned".to_string())?;
            *guard = Some(RunningServer {
                shutdown: shutdown_tx,
                handle,
                host: cfg.host.clone(),
                port: cfg.port,
            });
        }
        Ok(self.info())
    }

    /// 停止（幂等）：等待监听端口释放。
    async fn stop(&self) -> Result<ServerInfo, String> {
        self.shutdown_running().await;
        Ok(self.info())
    }

    /// 停止当前监听并等待其结束：先发优雅退出信号，给一个短暂窗口；超时则强制 abort，
    /// 确保 serve 任务（及其 listener）被销毁后再返回。
    async fn shutdown_running(&self) {
        let running = self.inner.lock().ok().and_then(|mut guard| guard.take());
        let Some(running) = running else {
            return;
        };
        let _ = running.shutdown.send(());
        let abort = running.handle.abort_handle();
        let mut handle = running.handle;
        if tokio::time::timeout(GRACEFUL_SHUTDOWN_TIMEOUT, &mut handle)
            .await
            .is_err()
        {
            // 仍有长连接（如 SSE）未结束：强制取消，释放端口。
            abort.abort();
            let _ = handle.await;
        }
    }

    /// 读取 `config.json` 的 `server` 节（不存在则为 `None`）。
    fn read_section(&self) -> Option<ServerSection> {
        ConfigStore::new(self.storage_root.clone())
            .read()
            .ok()
            .and_then(|config| config.server)
    }

    /// env > config 解析生效的监听参数。
    /// 绑定地址由配置 `lan` 派生（`host` 字段仅 headless 用，桌面端不参与，避免与 `lan` 冲突）。
    fn resolve_config(&self) -> ServerConfig {
        let (env_host, env_port, env_token) = server_env_overrides();
        let section = self.read_section();
        let lan = section.as_ref().and_then(|s| s.lan).unwrap_or(false);
        ServerConfig {
            host: env_host.unwrap_or_else(|| {
                if lan {
                    LAN_HOST.to_string()
                } else {
                    LOOPBACK_HOST.to_string()
                }
            }),
            port: env_port
                .or_else(|| section.as_ref().and_then(|s| s.port))
                .unwrap_or(DEFAULT_SERVER_PORT),
            tokens: env_token
                .map(|t| vec![t])
                .or_else(|| section.as_ref().and_then(|s| s.tokens.clone()))
                .unwrap_or_default(),
        }
    }

    /// 持久化 `server` 节：经 `ConfigStore` 原子读改写，保留其余顶层键。
    fn persist<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut ServerSection),
    {
        ConfigStore::new(self.storage_root.clone())
            .update(|config| f(config.server.get_or_insert_with(Default::default)))
            .map_err(|error| error.to_string())
    }
}

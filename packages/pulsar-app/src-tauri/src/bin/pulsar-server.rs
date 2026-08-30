//! pulsar-server：headless 服务器版。
//!
//! 无 GUI / 无 WebView，直接启动内嵌网络服务（RPC / SSE / WS + 前端静态托管）。
//! 配置默认来自 `<storage_root>/.pulsar/config.json` 的 `server` 节（与 GUI 一致），
//! 可用 CLI 参数覆盖：`--host` / `--port` / `--token`。
//! `storage_root` 默认当前工作目录下的 `.pulsar`（与 pulsar-cli / pulsar-tui 一致）。

use std::{path::PathBuf, sync::Arc};

use pulsar_app_lib::core::{
    app_log,
    config::{server_env_overrides, ConfigStore, DEFAULT_SERVER_HOST, DEFAULT_SERVER_PORT},
    log_phase::PHASE_NEURON_BOOTSTRAP_NEURONS,
    storage, StateChange, StateEmitter,
};
use pulsar_app_lib::net::{run_server, NetState, ServerConfig};
use pulsar_app_lib::server_runtime;
use pulsar_app_lib::terminal::TerminalEventHub;
use tokio::sync::broadcast;

fn print_usage() {
    println!(
        "pulsar-server: headless network server for Pulsar (no GUI)\n\n\
         Usage: pulsar-server [OPTIONS]\n\n\
         Options:\n\
           --host <addr>    listen address (default: config.json or 127.0.0.1)\n\
           --port <port>    listen port   (default: config.json or 9999)\n\
           --token <token>  bearer token whitelist (overrides config.json)\n\
           -h, --help       print this help\n\n\
         Data root: `<cwd>/.pulsar` (config.json `server` section is read from there)."
    );
}

#[tokio::main]
async fn main() {
    // CLI 覆盖参数（轻量解析，不引入 clap）。
    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut token: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => host = args.next(),
            "--port" => port = args.next().and_then(|p| p.parse().ok()),
            "--token" => token = args.next(),
            "-h" | "--help" => {
                print_usage();
                return;
            }
            other => {
                eprintln!("unknown argument: {other}");
                print_usage();
                std::process::exit(2);
            }
        }
    }

    let storage_root = storage::default_root().unwrap_or_else(|_| PathBuf::from("."));
    if let Err(error) = app_log::init(&storage_root, None, true) {
        eprintln!("warning: failed to init logging: {error}");
    }
    tracing::info!(path = %storage_root.display(), "pulsar-server starting");

    // 覆盖链：CLI > env(PULSAR_HOST/PORT/TOKEN) > config.json `server` 节 > 内置默认。
    let (env_host, env_port, env_token) = server_env_overrides();
    let host = host.or(env_host);
    let port = port.or(env_port);
    let token = token.or(env_token);

    // server 配置：config.json `server` 节 + CLI/env 覆盖。
    let cfg = ConfigStore::new(storage_root.clone())
        .read()
        .ok()
        .and_then(|config| config.server)
        .map(|section| ServerConfig {
            host: host
                .clone()
                .unwrap_or_else(|| section.host.unwrap_or_else(|| DEFAULT_SERVER_HOST.into())),
            port: port.unwrap_or_else(|| section.port.unwrap_or(DEFAULT_SERVER_PORT)),
            tokens: token
                .clone()
                .map(|t| vec![t])
                .or(section.tokens)
                .unwrap_or_default(),
        })
        .unwrap_or_else(|| ServerConfig {
            host: host.unwrap_or_else(|| DEFAULT_SERVER_HOST.into()),
            port: port.unwrap_or(DEFAULT_SERVER_PORT),
            tokens: token.map(|t| vec![t]).unwrap_or_default(),
        });

    // headless 状态发射：仅广播到 SSE（`/events`），无桌面 IPC 事件。
    let (events_tx, _) = broadcast::channel::<StateChange>(256);
    let events_tx_for_emit = events_tx.clone();
    let state_emit: StateEmitter = Arc::new(move |change: StateChange| {
        let _ = events_tx_for_emit.send(change);
    });

    let terminal_hub = TerminalEventHub::new_headless();
    let runtime = match server_runtime::build_server_runtime(
        &storage_root,
        state_emit.clone(),
        terminal_hub.clone(),
    ) {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(error = %error, "failed to build server runtime");
            eprintln!("failed to init server runtime: {error}");
            std::process::exit(1);
        }
    };

    // bootstrap neurons（与 GUI 一致；不持有任何 Gateway 锁跨模型调用）。
    let neuron_manager = runtime.neuron_manager.clone();
    tokio::spawn(async move {
        match neuron_manager.bootstrap().await {
            Ok(report) => {
                tracing::info!(
                    phase = PHASE_NEURON_BOOTSTRAP_NEURONS,
                    create_neuron_id = %report.create_neuron_id,
                    select_neuron_id = %report.select_neuron_id,
                    "neuron bootstrap complete"
                );
            }
            Err(error) => {
                tracing::warn!(
                    phase = PHASE_NEURON_BOOTSTRAP_NEURONS,
                    error_code = error.code(),
                    error = %error,
                    "neuron bootstrap incomplete"
                );
            }
        }
    });

    let net_state = NetState {
        gateway: runtime.gateway.clone(),
        state_emit: state_emit.clone(),
        events_tx: events_tx.clone(),
        tokens: cfg.tokens.clone(),
        terminal: runtime.terminal_manager.clone(),
        terminal_hub: runtime.terminal_hub.clone(),
        host: cfg.host.clone(),
        port: cfg.port,
    };

    tracing::info!(
        addr = format!("{}:{}", cfg.host, cfg.port),
        token_count = cfg.tokens.len(),
        "network server starting (headless)"
    );
    if let Err(error) = run_server(cfg, net_state).await {
        tracing::error!(error = %error, "network server exited");
        eprintln!("network server error: {error}");
        std::process::exit(1);
    }
}

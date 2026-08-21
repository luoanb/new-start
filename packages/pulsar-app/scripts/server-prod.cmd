@echo off
rem 局域网生产模式启动 pulsar-server：监听 0.0.0.0:9999（Windows cmd）
cd /d "%~dp0.."
set PULSAR_HOST=0.0.0.0
set PULSAR_PORT=9999
cargo run --release --manifest-path src-tauri\Cargo.toml --bin pulsar-server --features embed-static --

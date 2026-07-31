# Micro-spec: agent-app Wayland IME candidate offset

## Goal

Fix fcitx5 candidate window severe offset (worse near bottom inputs) in `packages/agent-app` on Ubuntu Wayland.

## Done Contract

- Normal `pnpm tauri dev` (no manual env) → topic + chat IME popup near caret.
- Only affects Linux Wayland when `GTK_IM_MODULE` would otherwise force fcitx GTK IM module.
- X11 / forced `GDK_BACKEND=x11` unchanged.

## Fix

Before GTK/Tauri init: if `WAYLAND_DISPLAY` is set and `GDK_BACKEND` is not `x11`, remove `GTK_IM_MODULE` so WebKitGTK uses native text-input.

## Evidence

- `GDK_BACKEND=x11` and `env -u GTK_IM_MODULE` both fixed positioning in user A/B.
- `display: contents` change alone did not; offsets varied by input Y position.

## Change Log

- 2026-07-31: Implemented in `packages/agent-app/src-tauri/src/main.rs` (`prepare_linux_wayland_ime_env`). `cargo check` OK. Awaiting user retest with plain `pnpm tauri dev`.

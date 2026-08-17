//! 脚本运行时（独立子系统，与 `core`/`net`/`tui` 平行）。
//!
//! 定位：「类 Node.js 运行时」的第一步——让 pulsar 具备执行脚本代码的最小能力。
//! 本模块只负责"执行代码"，宿主 API / 事件订阅 / 模块系统 / 脚本文件管理 / 存储
//! 一律后置（见 micro-spec 2026-08-09_14-30_script-runtime-mlua-bootstrap）。

pub mod script_engine;

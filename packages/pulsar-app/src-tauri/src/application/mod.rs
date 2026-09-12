//! 应用驱动层 + 组合根：编排核心轮次协议、装配扩展实现、承载业务副作用。

pub mod agent_session;
pub mod assistant_session;
pub mod chat_session;
pub mod drivers;
pub mod gateway;
pub mod hook;
pub mod insert_catalog;
pub mod poller;
pub mod poller_step;
pub mod session_tracker;

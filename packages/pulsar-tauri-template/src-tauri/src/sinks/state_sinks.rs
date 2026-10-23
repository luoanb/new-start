//! 状态事件出口：把领域事件映射为前端 `StateChange` 广播。
//!
//! 本包为功能无关的应用骨架，出口只保留「广播一个 `StateChange`」这一极简形态，
//! 不引入任何领域事件契约（`DomainEvent` / `EventSink` 等属业务层）。

use crate::core::events::{StateChange, StateEmitter};

/// 事件出口真实实现：包装 `StateEmitter`（写操作完成后广播 `StateChange`，
/// 前端经桌面 IPC 消费）。
#[derive(Clone)]
pub struct StateEventSink {
    emitter: StateEmitter,
}

impl StateEventSink {
    pub fn new(emitter: StateEmitter) -> Self {
        Self { emitter }
    }

    /// 广播一条状态变更。
    pub fn publish(&self, change: StateChange) {
        (self.emitter)(change);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn state_event_sink_publishes_change() {
        let seen: Arc<Mutex<Vec<StateChange>>> = Default::default();
        let sink_seen = Arc::clone(&seen);
        let emitter: StateEmitter = Arc::new(move |change: StateChange| {
            sink_seen.lock().unwrap().push(change);
        });
        let sink = StateEventSink::new(emitter);
        sink.publish(StateChange::AppReady);
        let events = seen.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], StateChange::AppReady));
    }
}

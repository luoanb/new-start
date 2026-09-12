//! 状态事件出口：把领域事件映射为前端 `StateChange` 广播。

use crate::core::events::{StateChange, StateEmitter};
use crate::core::round_contract::{DomainEvent, EventSink};

/// 事件出口真实实现：包装现有 `StateEmitter`（写操作完成后广播 `StateChange`，
/// GUI 经桌面 IPC、headless 经 SSE broadcast）。
#[derive(Clone)]
pub struct StateEventSink {
    emitter: StateEmitter,
}

impl StateEventSink {
    pub fn new(emitter: StateEmitter) -> Self {
        Self { emitter }
    }
}

impl EventSink for StateEventSink {
    fn publish(&self, event: DomainEvent) {
        match event {
            DomainEvent::Fact(change) => (self.emitter)(change),
            DomainEvent::Delta {
                conversation_id,
                delta,
            } => (self.emitter)(StateChange::MessageDelta {
                conversation_id,
                message_index: delta.message_index,
                content: delta.content,
                reasoning: delta.reasoning,
                done: delta.done,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn state_event_sink_maps_fact_and_delta() {
        let seen: Arc<Mutex<Vec<StateChange>>> = Default::default();
        let sink_seen = Arc::clone(&seen);
        let emitter: StateEmitter = Arc::new(move |change: StateChange| {
            sink_seen.lock().unwrap().push(change);
        });
        let sink = StateEventSink::new(emitter);
        sink.publish(DomainEvent::Fact(StateChange::Conversations {
            affected: vec!["c1".into()],
        }));
        sink.publish(DomainEvent::Delta {
            conversation_id: "c1".into(),
            delta: crate::core::round_contract::StreamDelta {
                message_index: 3,
                content: "partial".into(),
                reasoning: String::new(),
                done: false,
            },
        });
        let events = seen.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(
            matches!(&events[0], StateChange::Conversations { affected } if affected == &vec!["c1".to_string()]),
            "Fact 原样透传"
        );
        assert!(
            matches!(&events[1], StateChange::MessageDelta { message_index: 3, done: false, .. }),
            "Delta 映射为 wire 的 MessageDelta 事件"
        );
    }
}

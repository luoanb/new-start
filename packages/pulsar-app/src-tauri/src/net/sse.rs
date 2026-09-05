//! SSE 事件推送：把后端 `StateChange` 广播给远程前端。
//!
//! 复用 Tauri 本机通道同一 `StateChange` 序列化格式与事件名
//! （`app://state-changed`），前端 SSE handler 可与 Tauri `listen` handler 复用。

use std::convert::Infallible;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{stream, Stream};

use crate::core::events::STATE_CHANGED_EVENT;

use super::NetState;

pub async fn handle_sse(
    State(state): State<NetState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.events_tx.subscribe();
    let stream = stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(change) => {
                    let data = serde_json::to_string(&change).unwrap_or_default();
                    return Some((
                        Ok::<_, Infallible>(
                            Event::default()
                                .event(STATE_CHANGED_EVENT)
                                .data(data),
                        ),
                        rx,
                    ));
                }
                // Lagged / Closed：跳过丢失项继续；Closed 时结束流。
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

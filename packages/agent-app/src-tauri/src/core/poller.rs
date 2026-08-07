use std::{
    fmt,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use serde::{Deserialize, Serialize};

use super::{
    config::ConfigStore,
    error::AppResult,
};

/// Defaults when `config.json` omits `poller` fields.
pub const DEFAULT_POLLER_BASE_INTERVAL_MS: u64 = 1000;
/// 默认每 5 个基础 tick 推进一轮课题（1s × 5 = 5 秒），可被 config.json 覆盖。
pub const DEFAULT_ASSISTANT_POLL_TICKS: u64 = 5;
pub const DEFAULT_ASSISTANT_POLL_PARALLELISM: u64 = 2;
/// 单次 PollAll 内同时推进的课题数上限，避免并发过高打爆模型 API。
pub const MAX_ASSISTANT_POLL_PARALLELISM: u64 = 8;

/// 轮询并发推进数量的共享运行时值：Gateway 建好后再分发给
/// Poller（状态展示）与 AssistantMode（实际执行），两边读写同一原子。
pub type SharedPollParallelism = Arc<AtomicUsize>;

pub fn new_shared_poll_parallelism(initial: usize) -> SharedPollParallelism {
    Arc::new(AtomicUsize::new(initial.max(1)))
}

/// Startup settings loaded from `.agent-app/config.json` → `poller`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollerSettings {
    /// When true, scheduler starts in Running; otherwise Paused.
    pub enabled: bool,
    pub base_interval_ms: u64,
    pub assistant_interval_ticks: u64,
    /// 单次 PollAll 内同时推进的课题数上限。
    pub assistant_poll_parallelism: u64,
}

impl Default for PollerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            base_interval_ms: DEFAULT_POLLER_BASE_INTERVAL_MS,
            assistant_interval_ticks: DEFAULT_ASSISTANT_POLL_TICKS,
            assistant_poll_parallelism: DEFAULT_ASSISTANT_POLL_PARALLELISM,
        }
    }
}

/// Reads / writes poller settings from `{storage_root}/config.json`
/// via the shared `ConfigStore` (read-modify-write, lossless).
pub struct PollerConfigReader {
    store: ConfigStore,
}

impl PollerConfigReader {
    pub fn new(storage_root: PathBuf) -> Self {
        Self {
            store: ConfigStore::new(storage_root),
        }
    }

    pub fn load(&self) -> AppResult<PollerSettings> {
        let config = self.store.read()?;
        let section = config.poller.unwrap_or_default();
        Ok(PollerSettings {
            enabled: section.enabled.unwrap_or(false),
            base_interval_ms: section
                .base_interval_ms
                .unwrap_or(DEFAULT_POLLER_BASE_INTERVAL_MS)
                .max(1),
            assistant_interval_ticks: section
                .assistant_interval_ticks
                .unwrap_or(DEFAULT_ASSISTANT_POLL_TICKS)
                .max(1),
            assistant_poll_parallelism: section
                .assistant_poll_parallelism
                .unwrap_or(DEFAULT_ASSISTANT_POLL_PARALLELISM)
                .clamp(1, MAX_ASSISTANT_POLL_PARALLELISM),
        })
    }

    /// 持久化并发数量（clamp 到 `[1, MAX_ASSISTANT_POLL_PARALLELISM]`）。
    pub fn set_parallelism(&self, n: u64) -> AppResult<()> {
        let clamped = n.clamp(1, MAX_ASSISTANT_POLL_PARALLELISM);
        self.store.update(|config| {
            let section = config.poller.get_or_insert_with(Default::default);
            section.assistant_poll_parallelism = Some(clamped);
        })
    }
}

/// Handler invoked by the generic Poller when a task is due.
pub trait PollHandler: Send {
    fn on_tick(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PollerRunState {
    Running,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollerStatus {
    pub state: PollerRunState,
    pub tick_count: u64,
    pub base_interval_ms: u64,
    pub task_count: usize,
    pub pending_trigger: bool,
    /// 单次 PollAll 内同时推进的课题数上限（运行时值）。
    pub assistant_poll_parallelism: u64,
}

struct PollTask {
    name: String,
    interval_ticks: u64,
    last_fired_tick: u64,
    handler: Box<dyn PollHandler>,
}

/// Generic tick scheduler. Business logic lives only inside registered handlers.
pub struct Poller {
    base_interval_ms: u64,
    tick_count: u64,
    state: PollerRunState,
    pending_trigger: bool,
    tasks: Vec<PollTask>,
    /// 与 AssistantMode 共享的并发推进数量（运行时可变）。
    poll_parallelism: SharedPollParallelism,
}

impl fmt::Debug for Poller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Poller")
            .field("base_interval_ms", &self.base_interval_ms)
            .field("tick_count", &self.tick_count)
            .field("state", &self.state)
            .field("pending_trigger", &self.pending_trigger)
            .field("task_count", &self.tasks.len())
            .finish()
    }
}

impl Poller {
    pub fn new(base_interval_ms: u64, poll_parallelism: SharedPollParallelism) -> Self {
        Self {
            base_interval_ms: base_interval_ms.max(1),
            tick_count: 0,
            // Default off: user advances via manual step / poll_trigger; resume to enable auto.
            state: PollerRunState::Paused,
            pending_trigger: false,
            tasks: Vec::new(),
            poll_parallelism,
        }
    }

    /// 更新并发推进数量（clamp 到 ≥1），返回实际生效值。
    pub fn set_parallelism(&self, n: usize) -> usize {
        let n = n.max(1);
        self.poll_parallelism.store(n, Ordering::Relaxed);
        n
    }

    pub fn register(
        &mut self,
        name: &str,
        interval_ticks: u64,
        handler: Box<dyn PollHandler>,
    ) -> AppResult<()> {
        let interval_ticks = interval_ticks.max(1);
        if let Some(existing) = self.tasks.iter_mut().find(|task| task.name == name) {
            existing.interval_ticks = interval_ticks;
            existing.handler = handler;
            return Ok(());
        }
        self.tasks.push(PollTask {
            name: name.to_string(),
            interval_ticks,
            last_fired_tick: self.tick_count,
            handler,
        });
        Ok(())
    }

    pub fn tick(&mut self) {
        if self.state == PollerRunState::Paused && !self.pending_trigger {
            return;
        }

        self.tick_count = self.tick_count.saturating_add(1);
        let force_all = self.pending_trigger;
        self.pending_trigger = false;
        if force_all {
            tracing::info!(phase = "poller_tick", tick = self.tick_count, "trigger consumed: force_all firing all handlers");
        }

        for task in &mut self.tasks {
            let due = force_all
                || self.tick_count.saturating_sub(task.last_fired_tick) >= task.interval_ticks;
            if !due {
                continue;
            }
            task.last_fired_tick = self.tick_count;
            // Handler errors must not break the scheduler.
            task.handler.on_tick();
        }
    }

    pub fn start(&mut self) {
        self.state = PollerRunState::Running;
    }

    pub fn pause(&mut self) {
        self.state = PollerRunState::Paused;
    }

    pub fn resume(&mut self) {
        self.state = PollerRunState::Running;
    }

    pub fn trigger(&mut self) {
        self.pending_trigger = true;
    }

    pub fn status(&self) -> PollerStatus {
        PollerStatus {
            state: self.state,
            tick_count: self.tick_count,
            base_interval_ms: self.base_interval_ms,
            task_count: self.tasks.len(),
            pending_trigger: self.pending_trigger,
            assistant_poll_parallelism: self.poll_parallelism.load(Ordering::Relaxed) as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    fn test_poller(interval_ms: u64) -> Poller {
        Poller::new(interval_ms, new_shared_poll_parallelism(2))
    }

    struct CountingHandler {
        counter: Arc<Mutex<u64>>,
    }

    impl PollHandler for CountingHandler {
        fn on_tick(&mut self) {
            *self.counter.lock().unwrap() += 1;
        }
    }

    #[test]
    fn fires_on_interval_and_respects_pause() {
        let counter = Arc::new(Mutex::new(0u64));
        let mut poller = test_poller(1000);
        poller.resume();
        poller
            .register(
                "assistant",
                3,
                Box::new(CountingHandler {
                    counter: Arc::clone(&counter),
                }),
            )
            .unwrap();

        poller.tick();
        poller.tick();
        assert_eq!(*counter.lock().unwrap(), 0);
        poller.tick();
        assert_eq!(*counter.lock().unwrap(), 1);

        poller.pause();
        poller.tick();
        poller.tick();
        poller.tick();
        assert_eq!(*counter.lock().unwrap(), 1);

        poller.resume();
        poller.tick();
        poller.tick();
        poller.tick();
        assert_eq!(*counter.lock().unwrap(), 2);
    }

    #[test]
    fn config_reader_defaults_and_overrides() {
        let root = std::env::temp_dir().join(format!(
            "agent-app-poller-config-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let reader = PollerConfigReader::new(root.clone());
        assert_eq!(reader.load().unwrap(), PollerSettings::default());

        fs::write(
            root.join("config.json"),
            r#"{"poller":{"enabled":true,"base_interval_ms":500,"assistant_interval_ticks":10}}"#,
        )
        .unwrap();
        let loaded = reader.load().unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.base_interval_ms, 500);
        assert_eq!(loaded.assistant_interval_ticks, 10);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn default_new_is_paused_until_resume() {
        let counter = Arc::new(Mutex::new(0u64));
        let mut poller = test_poller(1000);
        poller
            .register(
                "assistant",
                1,
                Box::new(CountingHandler {
                    counter: Arc::clone(&counter),
                }),
            )
            .unwrap();
        poller.tick();
        poller.tick();
        assert_eq!(*counter.lock().unwrap(), 0);
        assert_eq!(poller.status().state, PollerRunState::Paused);
    }

    #[test]
    fn trigger_fires_all_on_next_tick() {
        let counter = Arc::new(Mutex::new(0u64));
        let mut poller = test_poller(1000);
        poller
            .register(
                "assistant",
                100,
                Box::new(CountingHandler {
                    counter: Arc::clone(&counter),
                }),
            )
            .unwrap();

        // Default paused: trigger still forces a run on next tick.
        poller.trigger();
        assert_eq!(*counter.lock().unwrap(), 0);
        poller.tick();
        assert_eq!(*counter.lock().unwrap(), 1);

        poller.pause();
        poller.trigger();
        poller.tick();
        assert_eq!(*counter.lock().unwrap(), 2);
    }
}

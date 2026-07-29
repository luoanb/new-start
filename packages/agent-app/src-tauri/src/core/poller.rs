use std::fmt;

use super::error::AppResult;

/// Handler invoked by the generic Poller when a task is due.
pub trait PollHandler: Send {
    fn on_tick(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollerRunState {
    Running,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollerStatus {
    pub state: PollerRunState,
    pub tick_count: u64,
    pub base_interval_ms: u64,
    pub task_count: usize,
    pub pending_trigger: bool,
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
    pub fn new(base_interval_ms: u64) -> Self {
        Self {
            base_interval_ms: base_interval_ms.max(1),
            tick_count: 0,
            state: PollerRunState::Running,
            pending_trigger: false,
            tasks: Vec::new(),
        }
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

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
        let mut poller = Poller::new(1000);
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
    fn trigger_fires_all_on_next_tick() {
        let counter = Arc::new(Mutex::new(0u64));
        let mut poller = Poller::new(1000);
        poller
            .register(
                "assistant",
                100,
                Box::new(CountingHandler {
                    counter: Arc::clone(&counter),
                }),
            )
            .unwrap();

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

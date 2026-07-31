//! Runtime logging: rolling files + in-memory ring buffer (+ optional Tauri emit).

use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock,
    },
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{field::Visit, Event, Level, Subscriber};
use tracing_subscriber::{
    fmt,
    layer::{Context, Layer, SubscriberExt},
    registry::LookupSpan,
    reload, util::SubscriberInitExt, EnvFilter, Registry,
};

pub const LOG_EVENT: &str = "app://logs";
const DEFAULT_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_MAX_FILES: usize = 5;
const DEFAULT_RING_CAPACITY: usize = 2000;

static CONTROLS: OnceLock<LogControls> = OnceLock::new();

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

impl From<Level> for LogLevel {
    fn from(level: Level) -> Self {
        match level {
            Level::ERROR => Self::Error,
            Level::WARN => Self::Warn,
            Level::INFO => Self::Info,
            Level::DEBUG => Self::Debug,
            Level::TRACE => Self::Trace,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub ts_ms: u64,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fields: HashMap<String, String>,
}

#[derive(Clone)]
pub struct LogControls {
    buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    reload: reload::Handle<EnvFilter, Registry>,
    level: Arc<Mutex<LogLevel>>,
    log_dir: PathBuf,
}

impl LogControls {
    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.buffer.lock().iter().cloned().collect()
    }

    pub fn clear_buffer(&self) {
        self.buffer.lock().clear();
    }

    pub fn level(&self) -> LogLevel {
        self.level.lock().clone()
    }

    pub fn set_level(&self, level: LogLevel) -> Result<(), String> {
        let filter = EnvFilter::new(level.as_str());
        self.reload
            .reload(filter)
            .map_err(|error| error.to_string())?;
        *self.level.lock() = level;
        Ok(())
    }

    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }
}

/// Initialize global logging. Safe to call once; later calls are no-ops.
///
/// `emit` is optional: pass a callback used by the UI layer (Tauri emit).
/// `to_stderr`: enable stderr fmt layer (CLI yes; TUI no to avoid corrupting the UI).
pub fn init(
    storage_root: &Path,
    emit: Option<Arc<dyn Fn(LogEntry) + Send + Sync>>,
    to_stderr: bool,
) -> Result<&'static LogControls, String> {
    if let Some(existing) = CONTROLS.get() {
        return Ok(existing);
    }

    let log_dir = storage_root.join("logs");
    fs::create_dir_all(&log_dir).map_err(|error| error.to_string())?;

    let file_writer = SizeRotatingWriter::open(
        &log_dir,
        "agent-app.log",
        DEFAULT_MAX_FILE_BYTES,
        DEFAULT_MAX_FILES,
    )
    .map_err(|error| error.to_string())?;

    let default_level = std::env::var("AGENT_APP_LOG")
        .ok()
        .and_then(|value| LogLevel::parse(&value))
        .unwrap_or(LogLevel::Info);

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(default_level.as_str()))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let (filter_layer, reload_handle) = reload::Layer::new(env_filter);

    let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(DEFAULT_RING_CAPACITY)));
    let controls = LogControls {
        buffer: Arc::clone(&buffer),
        reload: reload_handle,
        level: Arc::new(Mutex::new(default_level)),
        log_dir: log_dir.clone(),
    };

    let ui_layer = UiLogLayer {
        buffer: Arc::clone(&buffer),
        capacity: DEFAULT_RING_CAPACITY,
        emit,
    };

    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(move || file_writer.clone());

    let registry = Registry::default()
        .with(filter_layer)
        .with(file_layer)
        .with(ui_layer);

    if to_stderr {
        registry
            .with(fmt::layer().with_ansi(false).with_target(true))
            .try_init()
            .map_err(|error| error.to_string())?;
    } else {
        registry
            .try_init()
            .map_err(|error| error.to_string())?;
    }

    let _ = CONTROLS.set(controls);
    CONTROLS
        .get()
        .ok_or_else(|| "log controls missing after init".to_string())
}

pub fn controls() -> Option<&'static LogControls> {
    CONTROLS.get()
}

pub fn snapshot() -> Vec<LogEntry> {
    controls().map(|c| c.snapshot()).unwrap_or_default()
}

pub fn clear_buffer() {
    if let Some(c) = controls() {
        c.clear_buffer();
    }
}

pub fn get_level() -> String {
    controls()
        .map(|c| c.level().as_str().to_string())
        .unwrap_or_else(|| "info".to_string())
}

pub fn set_level(level: &str) -> Result<String, String> {
    let parsed = LogLevel::parse(level).ok_or_else(|| format!("invalid log level: {level}"))?;
    let controls = controls().ok_or_else(|| "logging not initialized".to_string())?;
    controls.set_level(parsed.clone())?;
    Ok(parsed.as_str().to_string())
}

pub fn log_dir() -> Option<PathBuf> {
    controls().map(|c| c.log_dir().to_path_buf())
}

struct UiLogLayer {
    buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    capacity: usize,
    emit: Option<Arc<dyn Fn(LogEntry) + Send + Sync>>,
}

impl<S> Layer<S> for UiLogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        let message = if visitor.message.is_empty() {
            event.metadata().name().to_string()
        } else {
            visitor.message
        };
        let entry = LogEntry {
            ts_ms: now_ms(),
            level: (*event.metadata().level()).into(),
            target: event.metadata().target().to_string(),
            message,
            fields: visitor.fields,
        };

        {
            let mut guard = self.buffer.lock();
            if guard.len() >= self.capacity {
                guard.pop_front();
            }
            guard.push_back(entry.clone());
        }

        if let Some(emit) = &self.emit {
            emit(entry);
        }
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: HashMap<String, String>,
}

impl Visit for FieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let rendered = format!("{value:?}");
        if field.name() == "message" {
            self.message = rendered;
        } else {
            self.fields.insert(field.name().to_string(), rendered);
        }
    }
}

#[derive(Clone)]
struct SizeRotatingWriter {
    inner: Arc<SizeRotatingInner>,
}

struct SizeRotatingInner {
    dir: PathBuf,
    file_name: String,
    max_bytes: u64,
    max_files: usize,
    file: Mutex<File>,
    size: AtomicU64,
}

impl SizeRotatingWriter {
    fn open(
        dir: &Path,
        file_name: &str,
        max_bytes: u64,
        max_files: usize,
    ) -> std::io::Result<Self> {
        let path = dir.join(file_name);
        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self {
            inner: Arc::new(SizeRotatingInner {
                dir: dir.to_path_buf(),
                file_name: file_name.to_string(),
                max_bytes,
                max_files,
                file: Mutex::new(file),
                size: AtomicU64::new(size),
            }),
        })
    }

    fn rotate_if_needed(&self, upcoming: u64) -> std::io::Result<()> {
        let current = self.inner.size.load(Ordering::Relaxed);
        if current + upcoming <= self.inner.max_bytes {
            return Ok(());
        }

        let mut file_guard = self.inner.file.lock();
        file_guard.flush()?;
        drop(file_guard);

        let base = self.inner.dir.join(&self.inner.file_name);
        if self.inner.max_files > 0 {
            let oldest = self
                .inner
                .dir
                .join(format!("{}.{}", self.inner.file_name, self.inner.max_files));
            let _ = fs::remove_file(oldest);
            for idx in (1..self.inner.max_files).rev() {
                let from = self
                    .inner
                    .dir
                    .join(format!("{}.{}", self.inner.file_name, idx));
                let to = self
                    .inner
                    .dir
                    .join(format!("{}.{}", self.inner.file_name, idx + 1));
                if from.exists() {
                    let _ = fs::rename(&from, &to);
                }
            }
            let first_archive = self.inner.dir.join(format!("{}.1", self.inner.file_name));
            if base.exists() {
                let _ = fs::rename(&base, &first_archive);
            }
        } else if base.exists() {
            let _ = fs::remove_file(&base);
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&base)?;
        *self.inner.file.lock() = file;
        self.inner.size.store(0, Ordering::Relaxed);
        Ok(())
    }
}

impl Write for SizeRotatingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.rotate_if_needed(buf.len() as u64)?;
        let mut file = self.inner.file.lock();
        let written = file.write(buf)?;
        self.inner
            .size
            .fetch_add(written as u64, Ordering::Relaxed);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.file.lock().flush()
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_rotating_writer_rolls_files() {
        let dir = std::env::temp_dir().join(format!("agent-app-log-test-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let mut writer = SizeRotatingWriter::open(&dir, "t.log", 32, 2).unwrap();
        writeln!(writer, "{}", "x".repeat(40)).unwrap();
        writeln!(writer, "hello").unwrap();
        assert!(dir.join("t.log").exists());
        assert!(dir.join("t.log.1").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn parse_levels() {
        assert_eq!(LogLevel::parse("INFO"), Some(LogLevel::Info));
        assert_eq!(LogLevel::parse("warn"), Some(LogLevel::Warn));
        assert!(LogLevel::parse("nope").is_none());
    }
}

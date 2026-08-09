use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiTaskKind {
    ModelCall,
    ToolCall,
    ConfigLoad,
    SessionLoad,
}

impl TuiTaskKind {
    pub fn label(&self) -> &'static str {
        match self {
            TuiTaskKind::ModelCall => "Model Call",
            TuiTaskKind::ToolCall => "Tool Call",
            TuiTaskKind::ConfigLoad => "Config Load",
            TuiTaskKind::SessionLoad => "Session Load",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiTaskStatus {
    Running,
    Done,
    Failed,
    Cancelled,
}

impl TuiTaskStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            TuiTaskStatus::Running => " \u{25cf}",
            TuiTaskStatus::Done => " \u{2713}",
            TuiTaskStatus::Failed => " \u{2717}",
            TuiTaskStatus::Cancelled => " \u{2296}",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TuiTaskBlock {
    pub id: String,
    pub kind: TuiTaskKind,
    pub label: String,
    pub status: TuiTaskStatus,
    pub started_at: Instant,
    pub finished_at: Option<Instant>,
    pub summary: Option<String>,
    pub details: Vec<String>,
    pub expanded: bool,
    pub cancellable: bool,
}

impl TuiTaskBlock {
    pub fn new(id: String, kind: TuiTaskKind, label: String) -> Self {
        Self {
            id,
            kind,
            label,
            status: TuiTaskStatus::Running,
            started_at: Instant::now(),
            finished_at: None,
            summary: None,
            details: Vec::new(),
            expanded: false,
            cancellable: false,
        }
    }

    pub fn elapsed_secs(&self) -> f64 {
        let end = self.finished_at.unwrap_or_else(Instant::now);
        end.duration_since(self.started_at).as_secs_f64()
    }

    pub fn done(&mut self, summary: String) {
        self.status = TuiTaskStatus::Done;
        self.finished_at = Some(Instant::now());
        self.summary = Some(summary);
    }

    pub fn fail(&mut self, summary: String) {
        self.status = TuiTaskStatus::Failed;
        self.finished_at = Some(Instant::now());
        self.summary = Some(summary);
    }

    pub fn elapsed_str(&self) -> String {
        let secs = self.elapsed_secs();
        if secs < 60.0 {
            format!("{secs:.1}s")
        } else {
            let mins = (secs / 60.0).floor();
            let rem = secs - mins * 60.0;
            format!("{mins:.0}m {rem:.0}s")
        }
    }
}

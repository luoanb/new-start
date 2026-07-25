use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::io;
use std::time::Duration;

/// Actions that can be triggered by user input.
#[derive(Debug, Clone)]
pub enum TuiAction {
    /// Raw key event forwarded to the active widget (e.g. TextArea for undo/redo).
    Key(KeyEvent),
    /// Submit the current input as a message.
    Submit,
    /// Create a new blank session and switch to it.
    NewSession,
    /// Move focus to the next pane.
    FocusNext,
    /// Move focus to the previous pane.
    FocusPrev,
    /// Scroll the chat area up.
    ScrollUp(u16),
    /// Scroll the chat area down.
    ScrollDown(u16),
    /// Select the highlighted session from the sessions list.
    SelectCurrentSession,
    /// Select the next item in a list.
    ListNext,
    /// Select the previous item in a list.
    ListPrev,
    /// Toggle help overlay.
    ToggleHelp,
    /// Toggle sessions list view.
    ToggleSessions,
    /// Toggle expansion of a task block.
    ToggleTaskExpand(usize),
    /// Dismiss any open overlay (help, sessions list, error banner) or go back.
    DismissOverlay,
    /// Exit the application.
    Exit,
    /// No action needed (e.g. key release events).
    Noop,
}

const POLL_TIMEOUT: Duration = Duration::from_millis(50);

/// Reads the next terminal event and converts it to a `TuiAction`.
///
/// Key events that are not intercepted at the app level are wrapped in
/// `TuiAction::Key` so that the focused widget (e.g. `TextArea`) can process
/// them natively — supporting undo/redo, cursor movement, delete, etc.
pub fn read_action() -> io::Result<TuiAction> {
    if !event::poll(POLL_TIMEOUT)? {
        return Ok(TuiAction::Noop);
    }

    let event = event::read()?;
    Ok(match event {
        // Only handle key-press events; release / repeat are ignored.
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            intercept_app_key(key)
        }
        Event::Paste(content) => {
            // Ratatui-textarea handles paste via its input() method when it
            // receives the constituent key events.  We forward the first char
            // as a synthetic KeyEvent; remaining chars arrive on subsequent
            // reads.
            if let Some(ch) = content.chars().next() {
                let synthetic = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
                TuiAction::Key(synthetic)
            } else {
                TuiAction::Noop
            }
        }
        _ => TuiAction::Noop,
    })
}

/// Intercept app-level keys; everything else passes through to the widget.
fn intercept_app_key(key: KeyEvent) -> TuiAction {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            TuiAction::Exit
        }
        KeyCode::Enter => TuiAction::Submit,
        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            TuiAction::NewSession
        }
        KeyCode::Tab if key.modifiers.is_empty() => TuiAction::FocusNext,
        KeyCode::BackTab => TuiAction::FocusPrev,
        KeyCode::Esc => TuiAction::DismissOverlay,
        KeyCode::Char('?') => TuiAction::ToggleHelp,
        // — Up/Down scrolling or list navigation (app.rs decides based on context) —
        KeyCode::Up => TuiAction::ScrollUp(1),
        KeyCode::Down => TuiAction::ScrollDown(1),
        KeyCode::PageUp => TuiAction::ScrollUp(10),
        KeyCode::PageDown => TuiAction::ScrollDown(10),
        // Everything else: forward to the active widget.
        _ => TuiAction::Key(key),
    }
}

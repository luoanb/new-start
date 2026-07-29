use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::app::{FocusPane, TuiApp, TuiMessageRole};
use super::task::TuiTaskStatus;
use crate::core::ConversationMode;

/// Main render function called each frame.
pub fn render(frame: &mut Frame, app: &TuiApp) {
    let area = frame.area();

    // Overall vertical layout: top bar, main content, input area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top status bar
            Constraint::Min(1),    // main content (chat + lists + overlays)
            Constraint::Length(3), // input area (min 3 lines for textarea)
        ])
        .split(area);

    render_top_bar(frame, chunks[0], app);
    render_main_content(frame, chunks[1], app);
    render_input_area(frame, chunks[2], app);
}

fn render_top_bar(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let model_label = app
        .active_model
        .as_ref()
        .map(|m| format!("{}/{}", m.provider_id, m.model_id))
        .unwrap_or_else(|| "no-model".to_string());

    let session_id = &app.active_session_id;
    let short_session = if session_id.len() > 12 {
        format!(
            "{}..{}",
            &session_id[..6],
            &session_id[session_id.len() - 4..]
        )
    } else {
        session_id.clone()
    };

    let tasks_running = app
        .tasks
        .iter()
        .filter(|t| t.status == TuiTaskStatus::Running)
        .count();
    let status_suffix = if tasks_running > 0 {
        format!(" | {} running", tasks_running)
    } else {
        String::new()
    };

    let bar_text = format!(
        " agent-app | {} | {}{} ",
        short_session, model_label, status_suffix
    );

    let bar_style = if app.focus == FocusPane::Input {
        Style::default().fg(Color::Cyan).bg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White).bg(Color::DarkGray)
    };

    let bar = Paragraph::new(bar_text)
        .style(bar_style)
        .alignment(Alignment::Left);
    frame.render_widget(bar, area);
}

fn render_main_content(frame: &mut Frame, area: Rect, app: &TuiApp) {
    // If help overlay is active, render it
    if app.show_help {
        render_help_overlay(frame, area);
        return;
    }

    // If sessions list is active, render it
    if app.show_sessions_list {
        render_sessions_list(frame, area, app);
        return;
    }

    // Otherwise render the chat area with messages, tasks, and errors
    render_chat_area(frame, area, app);
}

fn render_chat_area(frame: &mut Frame, area: Rect, app: &TuiApp) {
    // Collect all renderable items (messages + task blocks + error banner)
    let mut lines: Vec<Line> = Vec::new();

    // Render messages
    for msg in &app.messages {
        let role_span = match msg.role {
            TuiMessageRole::User => Span::styled(
                " You ",
                Style::default().fg(Color::Green).bg(Color::DarkGray),
            ),
            TuiMessageRole::Assistant => Span::styled(
                " Assistant ",
                Style::default().fg(Color::Cyan).bg(Color::DarkGray),
            ),
            TuiMessageRole::Tool => Span::styled(
                " Tool ",
                Style::default().fg(Color::Yellow).bg(Color::DarkGray),
            ),
            TuiMessageRole::Error => Span::styled(
                " Error ",
                Style::default().fg(Color::Red).bg(Color::DarkGray),
            ),
            TuiMessageRole::Status => Span::styled(
                " Status ",
                Style::default().fg(Color::Blue).bg(Color::DarkGray),
            ),
        };
        lines.push(Line::from(vec![role_span]));
        lines.push(Line::from(Span::raw(&msg.content)));
        lines.push(Line::from(""));
    }

    // Render task blocks
    for (_idx, task) in app.tasks.iter().enumerate() {
        let status_symbol = task.status.symbol();
        let (status_color, _border_color) = match task.status {
            TuiTaskStatus::Running => (Color::Yellow, Color::Yellow),
            TuiTaskStatus::Done => (Color::Green, Color::Green),
            TuiTaskStatus::Failed => (Color::Red, Color::Red),
            TuiTaskStatus::Cancelled => (Color::Gray, Color::DarkGray),
        };

        let title = format!(" {} {} ", task.kind.label(), status_symbol);
        let elapsed = task.elapsed_str();

        let header = Span::styled(
            format!("{title} | {elapsed}"),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        );
        lines.push(Line::from(header));
        lines.push(Line::from(Span::raw(format!("  {}", task.label))));

        if let Some(summary) = &task.summary {
            lines.push(Line::from(Span::raw(format!("  {summary}"))));
        }

        if task.expanded && !task.details.is_empty() {
            for detail in &task.details {
                lines.push(Line::from(Span::raw(format!("    {detail}"))));
            }
        }

        // Show expand hint if there are details
        if !task.details.is_empty() {
            let hint = if task.expanded {
                "  [press t to collapse]"
            } else {
                "  [press t to expand]"
            };
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(Color::DarkGray),
            )));
        }

        lines.push(Line::from(""));
    }

    // Render error banner
    if let Some(error) = &app.error_banner {
        lines.push(Line::from(Span::styled(
            format!(" Error [{}] ", error.code),
            Style::default()
                .fg(Color::Red)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::raw(format!(
            " What: {}",
            error.what_happened
        ))));
        for cause in &error.possible_causes {
            lines.push(Line::from(Span::styled(
                format!(" Cause: {cause}"),
                Style::default().fg(Color::Gray),
            )));
        }
        for action in &error.next_actions {
            lines.push(Line::from(Span::styled(
                format!(" -> {action}"),
                Style::default().fg(Color::Cyan),
            )));
        }
        lines.push(Line::from(Span::styled(
            " [Esc to dismiss]",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
    }

    // If no content, show a startup hint
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " Welcome to Agent App TUI",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Type /help for commands, /exit to quit.",
            Style::default().fg(Color::Gray),
        )));
        if app.active_model.is_none() {
            lines.push(Line::from(Span::styled(
                " Run /provider then /model <provider> <model> to select a chat model.",
                Style::default().fg(Color::Yellow),
            )));
        }
    }

    let text = Text::from(lines.clone());

    // scroll_offset = 0  → show bottom (newest messages)
    // scroll_offset > 0  → scrolled up by that many lines from the bottom
    // Use estimated wrapped line count (each logical line may wrap to multiple visual lines)
    let term_width = (area.width as usize).max(1);
    let estimated_wrapped: usize = lines
        .iter()
        .map(|l| {
            let w = l.width();
            if w == 0 {
                1
            } else {
                // Ceiling division: how many visual lines this logical line wraps to
                (w + term_width - 1) / term_width
            }
        })
        .sum();
    let visible_height = (area.height as usize).max(1);
    let max_scroll = estimated_wrapped.saturating_sub(visible_height);

    let effective_scroll = max_scroll.saturating_sub(app.scroll_offset as usize);

    let chat_block = Block::default().borders(Borders::NONE);

    let paragraph = Paragraph::new(text)
        .block(chat_block)
        .scroll((effective_scroll as u16, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn render_input_area(frame: &mut Frame, area: Rect, app: &TuiApp) {
    // Render suggestion popup (if active) above the input bar
    if app.show_suggestions && !app.suggestions.is_empty() {
        render_suggestions(frame, area, app);
    }

    let input_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(area);

    // Render the textarea widget
    let input_block = Block::default()
        .borders(Borders::TOP)
        .border_type(BorderType::Plain)
        .border_style(if app.focus == FocusPane::Input {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    // Clone the textarea for rendering (ratatui-textarea requires &mut)
    let mut textarea = app.input.clone();
    textarea.set_block(input_block);

    // Determine cursor visibility based on focus
    let cursor_style = if app.focus == FocusPane::Input {
        Style::default().fg(Color::White).bg(Color::DarkGray)
    } else {
        Style::default()
    };
    textarea.set_cursor_style(cursor_style);

    frame.render_widget(&textarea, input_area[0]);

    // Hint line
    let hint = if app.focus == FocusPane::Input {
        Span::styled(
            " Ctrl+J new session | Enter send | Tab focus | /help ",
            Style::default().fg(Color::DarkGray),
        )
    } else {
        Span::styled(
            " Press Tab to focus input ",
            Style::default().fg(Color::DarkGray),
        )
    };
    frame.render_widget(Paragraph::new(Line::from(hint)), input_area[1]);
}

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let lines: Vec<Line> = super::commands::cmd_help_text()
        .iter()
        .flat_map(|(cmd, desc)| {
            vec![Line::from(Span::styled(
                format!("  {cmd:<30} {desc}"),
                Style::default().fg(Color::White),
            ))]
        })
        .collect();

    let max_width = lines
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(60)
        .min(area.width.saturating_sub(4));

    let height = lines.len() as u16 + 4;

    let popup_x = (area.width.saturating_sub(max_width)) / 2;
    let popup_y = (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect {
        x: popup_x,
        y: popup_y,
        width: max_width.min(area.width),
        height: height.min(area.height),
    };

    frame.render_widget(Clear, popup_area);

    let help_block = Block::default()
        .title(" Help ")
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(Text::from(lines))
        .block(help_block)
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, popup_area);
}

fn render_sessions_list(frame: &mut Frame, area: Rect, app: &TuiApp) {
    // Get running session IDs for [Running] markers
    let running_ids: std::collections::HashSet<String> = app
        .gateway
        .session_tracker()
        .list()
        .ok()
        .map(|sessions| sessions.into_iter().map(|s| s.session_id).collect())
        .unwrap_or_default();

    let mut items: Vec<ListItem> = app
        .conversations
        .iter()
        .map(|conv| {
            let is_active = conv.id == app.active_session_id;
            let prefix = if is_active { "> " } else { "  " };
            let mode_tag = match conv.mode {
                ConversationMode::Chat => "[Chat]",
                ConversationMode::Agent => "[Agent]",
                ConversationMode::Assistant => "[Assistant]",
            };
            let running_tag = if running_ids.contains(&conv.id) {
                " [Running]"
            } else {
                ""
            };
            let msg_count = conv.messages.len();
            ListItem::new(format!(
                "{prefix}{mode_tag} {} ({} msgs){running_tag}",
                &conv.id[..conv.id.len().min(16)],
                msg_count
            ))
            .style(if is_active {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            })
        })
        .collect();

    // Add creation entries at the bottom
    let chat_new_idx = app.conversations.len();
    let agent_new_idx = app.conversations.len() + 1;
    let assistant_new_idx = app.conversations.len() + 2;
    let chat_selected = app.session_list_state.selected() == Some(chat_new_idx);
    let agent_selected = app.session_list_state.selected() == Some(agent_new_idx);
    let assistant_selected = app.session_list_state.selected() == Some(assistant_new_idx);

    items.push(
        ListItem::new(format!(
            "{}[+] New Chat session",
            if chat_selected { " > " } else { "   " }
        ))
        .style(if chat_selected {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        }),
    );
    items.push(
        ListItem::new(format!(
            "{}[+] New Agent session",
            if agent_selected { " > " } else { "   " }
        ))
        .style(if agent_selected {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        }),
    );
    items.push(
        ListItem::new(format!(
            "{}[+] New Assistant session",
            if assistant_selected { " > " } else { "   " }
        ))
        .style(if assistant_selected {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        }),
    );

    if items.is_empty() {
        items.push(ListItem::new("  No conversations").style(Style::default().fg(Color::Gray)));
    }

    let sessions_block = Block::default()
        .title(" Conversations ")
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let list = List::new(items)
        .block(sessions_block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(list, area, &mut app.session_list_state.clone());
}

/// Render command autocomplete suggestions above the input bar.
fn render_suggestions(frame: &mut Frame, input_area: Rect, app: &TuiApp) {
    let max_visible = 8.min(app.suggestions.len());
    let popup_height = max_visible as u16 + 2; // border top/bottom
    let popup_width = 60.min(input_area.width.saturating_sub(2));

    let popup_x = input_area.x + 1;
    let popup_y = input_area.y.saturating_sub(popup_height);

    let popup_area = Rect {
        x: popup_x,
        y: popup_y,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = app
        .suggestions
        .iter()
        .enumerate()
        .map(|(i, (cmd, desc))| {
            let is_selected = i == app.suggestion_index;
            let prefix = if is_selected { " > " } else { "   " };
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("{prefix}{cmd:<25} {desc}")).style(style)
        })
        .collect();

    let block = Block::default()
        .title(" Commands ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let mut state =
        ratatui::widgets::ListState::default().with_selected(Some(app.suggestion_index));
    frame.render_stateful_widget(list, popup_area, &mut state);
}

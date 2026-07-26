use ratatui::widgets::ListState;
use ratatui_textarea::TextArea;

use crate::core::{
    AppError, AppResult, ChatModelSelection, ChatOptions, Conversation, ConversationMode, Gateway,
    MessageRole, ModelInfo, ProviderInfo, RuntimeStatus,
};

use super::commands::{self, Command};
use super::error_view::TuiErrorView;
use super::event::TuiAction;
use super::render::render;
use super::task::{TuiTaskBlock, TuiTaskKind};
use super::TuiTerminal;

/// Role of a rendered chat message.
#[derive(Debug, Clone)]
pub enum TuiMessageRole {
    User,
    Assistant,
    Tool,
    Error,
    Status,
}

/// A rendered chat message in the TUI message list.
#[derive(Debug, Clone)]
pub struct TuiMessage {
    pub id: String,
    pub role: TuiMessageRole,
    pub content: String,
    pub timestamp: Option<u128>,
    pub collapsed: bool,
}

impl TuiMessage {
    fn user(content: String, id: String) -> Self {
        Self {
            id,
            role: TuiMessageRole::User,
            content,
            timestamp: None,
            collapsed: false,
        }
    }

    fn assistant(content: String, id: String) -> Self {
        Self {
            id,
            role: TuiMessageRole::Assistant,
            content,
            timestamp: None,
            collapsed: false,
        }
    }

    fn status(content: String) -> Self {
        Self {
            id: String::new(),
            role: TuiMessageRole::Status,
            content,
            timestamp: None,
            collapsed: false,
        }
    }
}

/// Which pane currently has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Input,
    Chat,
    SessionsList,
}

/// The main application state for the TUI.
pub struct TuiApp {
    pub gateway: Gateway,
    pub active_model: Option<ChatModelSelection>,
    pub active_session_id: String,
    pub messages: Vec<TuiMessage>,
    pub input: TextArea<'static>,
    pub input_history: Vec<String>,
    pub focus: FocusPane,
    pub tasks: Vec<TuiTaskBlock>,
    pub error_banner: Option<TuiErrorView>,
    pub should_quit: bool,
    pub show_help: bool,
    pub show_sessions_list: bool,
    pub conversations: Vec<Conversation>,
    pub status: RuntimeStatus,
    pub scroll_offset: u16,
    pub session_list_state: ListState,
    pub providers: Vec<ProviderInfo>,
    pub models: Vec<ModelInfo>,
    pub task_counter: u64,
    /// Command autocomplete suggestions
    pub show_suggestions: bool,
    pub suggestion_index: usize,
    pub suggestions: Vec<(String, String)>,
}

impl TuiApp {
    pub fn new(gateway: Gateway) -> AppResult<Self> {
        let status = gateway.status()?;
        let active_session_id = status.current_conversation_id.clone();
        let conversations = gateway.list_conversations()?;
        let active_model = gateway.default_model_selection()?;
        let providers = gateway.list_providers();

        // Load history for the active session
        let history = gateway.history(Some(active_session_id.clone())).unwrap_or_default();
        let mut messages = Vec::new();
        for (i, msg) in history.iter().enumerate() {
            let role = match msg.role {
                MessageRole::User => TuiMessageRole::User,
                MessageRole::Assistant => TuiMessageRole::Assistant,
                MessageRole::System => TuiMessageRole::Status,
                MessageRole::Compaction => TuiMessageRole::Status,
            };
            messages.push(TuiMessage {
                id: format!("{}-{i}", active_session_id),
                role,
                content: msg.content.clone(),
                timestamp: Some(msg.timestamp),
                collapsed: false,
            });
        }

        let input = TextArea::default();

        let session_list_state = ListState::default();

        let mut app = Self {
            gateway,
            active_model,
            active_session_id,
            messages,
            input,
            input_history: Vec::new(),
            focus: FocusPane::Input,
            tasks: Vec::new(),
            error_banner: None,
            should_quit: false,
            show_help: false,
            show_sessions_list: false,
            conversations,
            status,
            scroll_offset: 0,
            session_list_state,
            providers,
            models: Vec::new(),
            task_counter: 0,
            show_suggestions: false,
            suggestion_index: 0,
            suggestions: Vec::new(),
        };

        // Add a startup status message
        app.messages.push(TuiMessage::status(
            "Agent App TUI started.".to_string(),
        ));
        if app.active_model.is_none() {
            app.messages.push(TuiMessage::status(
                "No model selected. Use /provider then /model <provider> <model>.".to_string(),
            ));
        }

        Ok(app)
    }

    /// Main event loop. Owns the terminal and runs until quit.
    pub async fn run(&mut self, terminal: &mut TuiTerminal) -> AppResult<()> {
        loop {
            // Render current state
            terminal.draw(|frame| render(frame, self))?;

            // Check if we should quit
            if self.should_quit {
                break;
            }

            // Read and process the next action
            let action = super::event::read_action()
                .map_err(|e| AppError::RuntimeError(format!("Event read error: {e}")))?;

            self.update(action).await?;
        }

        Ok(())
    }

    /// Process a single TUI action.
    async fn update(&mut self, action: TuiAction) -> AppResult<()> {
        match action {
            TuiAction::Key(key) => {
                if self.focus == FocusPane::Input {
                    self.input.input(key);
                    self.update_suggestions();
                }
            }
            TuiAction::Submit => {
                if self.show_suggestions && !self.suggestions.is_empty() {
                    // Fill selected suggestion instead of submitting
                    self.fill_suggestion();
                } else if self.focus == FocusPane::Input {
                    let input_text = self.input.lines().join("\n");
                    let trimmed = input_text.trim().to_string();
                    if !trimmed.is_empty() {
                        self.input_history.push(input_text);
                        self.input = TextArea::default();

                        self.handle_submit(trimmed).await?;
                    }
                } else if self.show_sessions_list {
                    self.select_current_session();
                }
            }
            TuiAction::NewSession => {
                match self.create_new_session(ConversationMode::Chat) {
                    Ok(()) => {}
                    Err(error) => {
                        self.error_banner = Some(TuiErrorView::from(error));
                    }
                }
            }
            TuiAction::FocusNext => {
                self.focus = match self.focus {
                    FocusPane::Input => FocusPane::Chat,
                    FocusPane::Chat => FocusPane::Input,
                    FocusPane::SessionsList => FocusPane::Input,
                };
            }
            TuiAction::FocusPrev => {
                self.focus = match self.focus {
                    FocusPane::Input => FocusPane::Chat,
                    FocusPane::Chat => FocusPane::SessionsList,
                    FocusPane::SessionsList => FocusPane::Input,
                };
            }
            TuiAction::ScrollUp(amount) => {
                if self.show_suggestions && !self.suggestions.is_empty() {
                    if self.suggestion_index > 0 {
                        self.suggestion_index -= 1;
                    }
                } else if self.show_sessions_list {
                    let len = self.conversations.len() + 2;
                    if len > 0 {
                        let i = self.session_list_state.selected().unwrap_or(0);
                        self.session_list_state
                            .select(Some((i + len - 1) % len));
                    }
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_add(amount);
                }
            }
            TuiAction::ScrollDown(amount) => {
                if self.show_suggestions && !self.suggestions.is_empty() {
                    if self.suggestion_index + 1 < self.suggestions.len() {
                        self.suggestion_index += 1;
                    }
                } else if self.show_sessions_list {
                    let len = self.conversations.len() + 2;
                    if len > 0 {
                        let i = self.session_list_state.selected().unwrap_or(0);
                        self.session_list_state.select(Some((i + 1) % len));
                    }
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_sub(amount);
                }
            }
            TuiAction::ToggleHelp => {
                self.show_help = !self.show_help;
                if self.show_help {
                    self.show_sessions_list = false;
                }
            }
            TuiAction::ToggleSessions => {
                self.show_sessions_list = !self.show_sessions_list;
                if self.show_sessions_list {
                    let _ = self.refresh_conversations();
                    self.show_help = false;
                    self.focus = FocusPane::SessionsList;
                } else {
                    self.focus = FocusPane::Input;
                }
            }
            TuiAction::ToggleTaskExpand(idx) => {
                if idx < self.tasks.len() {
                    self.tasks[idx].expanded = !self.tasks[idx].expanded;
                }
            }
            TuiAction::DismissOverlay => {
                if self.show_suggestions {
                    self.show_suggestions = false;
                    self.suggestions.clear();
                } else if self.show_help {
                    self.show_help = false;
                } else if self.show_sessions_list {
                    self.show_sessions_list = false;
                    self.focus = FocusPane::Input;
                } else {
                    self.error_banner = None;
                }
            }
            TuiAction::SelectCurrentSession => {
                self.select_current_session();
            }
            TuiAction::ListNext => {
                if self.show_sessions_list {
                    let i = self.session_list_state.selected().unwrap_or(0);
                    let len = self.conversations.len();
                    if len > 0 {
                        self.session_list_state.select(Some((i + 1) % len));
                    }
                }
            }
            TuiAction::ListPrev => {
                if self.show_sessions_list {
                    let len = self.conversations.len();
                    if len > 0 {
                        let i = self.session_list_state.selected().unwrap_or(0);
                        self.session_list_state
                            .select(Some((i + len - 1) % len));
                    }
                }
            }
            TuiAction::Exit => {
                self.should_quit = true;
            }
            TuiAction::Noop => {}
        }

        Ok(())
    }

    /// Handle a submitted input line: either a command or a chat message.
    async fn handle_submit(&mut self, input: String) -> AppResult<()> {
        // Try parsing as a command
        if let Some(cmd) = Command::parse(&input) {
            self.execute_command(cmd).await;
            return Ok(());
        }

        // Not a command — send as a chat message
        if input.starts_with('/') {
            self.error_banner = Some(TuiErrorView::from(AppError::InvalidInput(format!(
                "Unknown command: {input}"
            ))));
            return Ok(());
        }

        self.send_chat_message(input).await;
        Ok(())
    }

    /// Execute a parsed command.
    async fn execute_command(&mut self, cmd: Command) {
        match cmd {
            Command::Help => {
                self.show_help = !self.show_help;
                if self.show_help {
                    self.show_sessions_list = false;
                }
            }
            Command::New => {
                if let Err(error) = self.create_new_session(ConversationMode::Chat) {
                    self.error_banner = Some(TuiErrorView::from(error));
                }
            }
            Command::NewAgent => {
                if let Err(error) = self.create_new_session(ConversationMode::Agent) {
                    self.error_banner = Some(TuiErrorView::from(error));
                }
            }
            Command::Skills => {
                let skills = self.gateway.list_skills();
                if skills.is_empty() {
                    self.messages
                        .push(TuiMessage::status("No skills available.".into()));
                } else {
                    let text = skills
                        .iter()
                        .map(|s| format!("  {} - {}", s.name, s.description))
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.messages.push(TuiMessage::status(format!(
                        "Skills:\n{text}"
                    )));
                }
            }
            Command::Providers => {
                let text = commands::cmd_provider_text(&self.providers);
                self.messages
                    .push(TuiMessage::status(format!("Providers:\n{text}")));
            }
            Command::Provider(provider_id) => {
                let provider = self.providers.iter().find(|p| p.id == provider_id);
                match provider {
                    Some(p) => {
                        self.messages.push(TuiMessage::status(format!(
                            "Provider: {} ({})\n  auth: {}\n  api_base: {:?}\n  kind: {:?}",
                            p.id, p.display_name, p.auth_env, p.api_base, p.kind
                        )));
                    }
                    None => {
                        self.messages.push(TuiMessage::status(format!(
                            "Provider not found: {provider_id}"
                        )));
                    }
                }
            }
            Command::Models(provider_id) => {
                match self.gateway.list_models(Some(provider_id.clone())) {
                    Ok(models) => {
                        let text = commands::cmd_models_text(&models);
                        self.messages
                            .push(TuiMessage::status(format!("Models:\n{text}")));
                    }
                    Err(error) => {
                        self.error_banner = Some(TuiErrorView::from(error));
                    }
                }
            }
            Command::Model(provider_id, model_id) => {
                match self.gateway.require_model(&provider_id, &model_id) {
                    Ok(()) => {
                        self.active_model = Some(ChatModelSelection {
                            provider_id: provider_id.clone(),
                            model_id: model_id.clone(),
                        });
                        self.messages.push(TuiMessage::status(format!(
                            "Selected model: {}/{}",
                            provider_id, model_id
                        )));
                    }
                    Err(error) => {
                        self.error_banner = Some(TuiErrorView::from(error));
                    }
                }
            }
            Command::Sessions => {
                self.show_sessions_list = !self.show_sessions_list;
                if self.show_sessions_list {
                    let _ = self.refresh_conversations();
                    self.show_help = false;
                    self.focus = FocusPane::SessionsList;
                }
            }
            Command::History => {
                match self.gateway.history(Some(self.active_session_id.clone())) {
                    Ok(history) => {
                        if history.is_empty() {
                            self.messages
                                .push(TuiMessage::status("No history.".into()));
                        } else {
                            let lines: Vec<String> = history
                                .iter()
                                .map(|msg| {
                                    let role = match msg.role {
                                        MessageRole::User => "user",
                                        MessageRole::Assistant => "assistant",
                                        MessageRole::System => "system",
                                        MessageRole::Compaction => "compaction",
                                    };
                                    format!("  [{role}] {}", msg.content)
                                })
                                .collect();
                            self.messages.push(TuiMessage::status(format!(
                                "History:\n{}",
                                lines.join("\n")
                            )));
                        }
                    }
                    Err(error) => {
                        self.error_banner = Some(TuiErrorView::from(error));
                    }
                }
            }
            Command::Clear => {
                match self.gateway.clear_conversation(Some(self.active_session_id.clone())) {
                    Ok(new_id) => {
                        self.messages.clear();
                        self.active_session_id = new_id;
                        self.messages
                            .push(TuiMessage::status("Conversation cleared.".into()));
                        let _ = self.refresh_conversations();
                    }
                    Err(error) => {
                        self.error_banner = Some(TuiErrorView::from(error));
                    }
                }
            }
            Command::Status => {
                match self.gateway.status() {
                    Ok(s) => {
                        let model_label = self
                            .active_model
                            .as_ref()
                            .map(|m| format!("{}/{}", m.provider_id, m.model_id))
                            .unwrap_or_else(|| "none".to_string());
                        self.messages.push(TuiMessage::status(format!(
                            "Status:\n  app: {}\n  storage: {}\n  session: {}\n  model: {}\n  skills: {}\n  conversations: {}",
                            s.app_name, s.storage_path, s.current_conversation_id,
                            model_label, s.skill_count, s.conversation_count
                        )));
                    }
                    Err(error) => {
                        self.error_banner = Some(TuiErrorView::from(error));
                    }
                }
            }
            Command::Config => {
                // Show current config info from status
                match self.gateway.status() {
                    Ok(s) => {
                        self.messages.push(TuiMessage::status(format!(
                            "Config:\n  storage: {}\n  session: {}",
                            s.storage_path, s.current_conversation_id
                        )));
                    }
                    Err(error) => {
                        self.error_banner = Some(TuiErrorView::from(error));
                    }
                }
            }
            Command::Call(provider_id, model_id, message) => {
                let task_id = format!("call-{}", self.next_task_id());
                let label = format!("calling {provider_id}/{model_id}");
                self.tasks
                    .push(TuiTaskBlock::new(task_id.clone(), TuiTaskKind::ModelCall, label));

                match self
                    .gateway
                    .call_model(crate::core::ModelCallRequest {
                        provider_id,
                        model_id,
                        messages: vec![crate::core::ModelMessage {
                            role: crate::core::ModelMessageRole::User,
                            content: message,
                            tool_calls: None,
                            tool_call_id: None,
                        }],
                        tools: None,
                    })
                    .await
                {
                    Ok(response) => {
                        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                            task.done("Response received".to_string());
                        }
                        self.messages
                            .push(TuiMessage::assistant(response.output, task_id));
                    }
                    Err(error) => {
                        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                            task.fail(error.to_string());
                        }
                        self.error_banner = Some(TuiErrorView::from(error));
                    }
                }
            }
            Command::Compact => {
                let task_id = format!("compact-{}", self.next_task_id());
                self.tasks.push(TuiTaskBlock::new(
                    task_id.clone(),
                    TuiTaskKind::SessionLoad,
                    "compacting conversation".to_string(),
                ));

                match self
                    .gateway
                    .compact_conversation(Some(self.active_session_id.clone()))
                    .await
                {
                    Ok(message) => {
                        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                            task.done("Compaction complete".to_string());
                        }
                        self.messages.push(TuiMessage::status(message));
                        // Reload messages to show the compaction summary
                        self.reload_messages();
                    }
                    Err(error) => {
                        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                            task.fail(error.to_string());
                        }
                        self.error_banner = Some(TuiErrorView::from(error));
                    }
                }
            }
            Command::Agent(cmd_msg) => {
                // Check the current conversation's mode from the conversations list
                let mode = self
                    .conversations
                    .iter()
                    .find(|c| c.id == self.active_session_id)
                    .map(|c| c.mode.clone())
                    .unwrap_or(crate::core::ConversationMode::Chat);
                match mode {
                    crate::core::ConversationMode::Chat => {
                        self.error_banner = Some(TuiErrorView::from(AppError::InvalidInput(
                            "当前会话为 Chat 模式，请使用 /new agent 创建 Agent 会话".into(),
                        )));
                    }
                    crate::core::ConversationMode::Agent => {
                        // Agent 模式：/agent <msg> 重定向到 send_chat_message
                        if !cmd_msg.is_empty() {
                            return self.send_chat_message(cmd_msg).await;
                        }
                        self.messages.push(TuiMessage::status(
                            "Agent 模式直接输入即可触发工具调用".into(),
                        ));
                    }
                }
            }
            Command::TopicAction(args) => {
                self.handle_topic_action(args);
            }
            Command::NeuronAction(args) => {
                self.handle_neuron_action(args);
            }
            Command::Exit => {
                self.should_quit = true;
            }
        }
    }

    /// Send a chat message to the current model.
    async fn send_chat_message(&mut self, input: String) {
        let Some(ref model) = self.active_model.clone() else {
            self.error_banner = Some(TuiErrorView::from(AppError::ModelNotSelected));
            return;
        };

        // Add user message to the display
        let user_msg_id = format!("user-{}", self.next_task_id());
        self.messages
            .push(TuiMessage::user(input.clone(), user_msg_id));

        // Create a task block for this model call
        let task_id = format!("call-{}", self.next_task_id());
        let label = format!("calling {}/{}", model.provider_id, model.model_id);
        self.tasks
            .push(TuiTaskBlock::new(task_id.clone(), TuiTaskKind::ModelCall, label));

        // Call the model (dispatches by conversation.mode)
        let result = self
            .gateway
            .send_model_message(
                &input,
                ChatOptions {
                    provider_id: model.provider_id.clone(),
                    model_id: model.model_id.clone(),
                    conversation_id: Some(self.active_session_id.clone()),
                },
            )
            .await;

        match result {
            Ok(response) => {
                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    task.done("Response received".to_string());
                }
                self.messages
                    .push(TuiMessage::assistant(response.response, task_id));
            }
            Err(error) => {
                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    task.fail(error.to_string());
                }
                self.error_banner = Some(TuiErrorView::from(error));
            }
        }

        // Refresh conversations list after sending
        let _ = self.refresh_conversations();

        // Auto-scroll to the latest message
        self.scroll_to_bottom();
    }

    /// Handle `/topics` list command.
    fn handle_topic_list(&mut self) {
        match self.gateway.topic_store() {
            Ok(store_arc) => match store_arc.lock() {
                Ok(store) => match store.list(None) {
                    Ok(topics) => {
                        if topics.is_empty() {
                            self.messages
                                .push(TuiMessage::status("No topics found.".into()));
                            return;
                        }
                        let mut lines = vec!["Topics:".to_string()];
                        for t in &topics {
                            let status = serde_json::to_string(&t.status)
                                .unwrap_or_default()
                                .trim_matches('"')
                                .to_string();
                            lines.push(format!(
                                "  [{:>3}%] {} - {} (id: {})",
                                t.progress, t.name, status, t.id
                            ));
                        }
                        self.messages.push(TuiMessage::status(lines.join("\n")));
                    }
                    Err(e) => {
                        self.error_banner = Some(TuiErrorView::from(e));
                    }
                },
                Err(e) => {
                    self.error_banner = Some(TuiErrorView::from(AppError::StorageError(
                        format!("Lock error: {}", e),
                    )));
                }
            },
            Err(e) => {
                self.error_banner = Some(TuiErrorView::from(e));
            }
        }
    }

    /// Handle `/topic <args>` commands.
    fn handle_topic_action(&mut self, args: Vec<String>) {
        if args.is_empty() {
            self.messages.push(TuiMessage::status(
                concat!(
                    "Topic commands:\n",
                    "  /topic list                 - List all topics\n",
                    "  /topic new <name>           - Create a new topic\n",
                    "  /topic <id>                 - View topic details\n",
                    "  /topic <id> set <f> <v>     - Update a field (name/status/description/progress)\n",
                    "  /topic <id> delete          - Delete a topic\n",
                    "\n",
                    "Status values: todo, in_progress, paused, done, cancelled"
                )
                .to_string(),
            ));
            return;
        }
        let action = args[0].as_str();
        match action {
            "list" => {
                self.handle_topic_list();
            }
            "new" if args.len() >= 2 => {
                let name = args[1..].join(" ");
                match self.gateway.topic_store() {
                    Ok(store_arc) => match store_arc.lock() {
                        Ok(store) => match store.create(
                            &name,
                            "",
                            crate::core::TopicStatus::Todo,
                            vec![],
                            None,
                        ) {
                            Ok(topic) => {
                                self.messages.push(TuiMessage::status(format!(
                                    "Created topic '{}' (id: {})",
                                    topic.name, topic.id
                                )));
                            }
                            Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(
                            AppError::StorageError(format!("Lock error: {}", e)),
                        )),
                    },
                    Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                }
            }
            id => {
                // Check if this is a simple view or a set/delete command
                if args.len() >= 3 && args[1] == "set" && args.len() >= 4 {
                    let field = args[2].as_str();
                    let value = args[3..].join(" ");
                    match self.gateway.topic_store() {
                        Ok(store_arc) => match store_arc.lock() {
                            Ok(store) => {
                                let mut update = crate::core::TopicUpdate::default();
                                match field {
                                    "name" => update.name = Some(value),
                                    "status" => {
                                        let json = format!("\"{}\"", value);
                                        if let Ok(s) =
                                            serde_json::from_str::<crate::core::TopicStatus>(&json)
                                        {
                                            update.status = Some(s);
                                        } else {
                                            self.error_banner = Some(TuiErrorView::from(
                                                AppError::InvalidInput(
                                                    "Invalid status".into(),
                                                ),
                                            ));
                                            return;
                                        }
                                    }
                                    "progress" => {
                                        if let Ok(p) = value.parse::<u8>() {
                                            update.progress = Some(p.min(100));
                                        }
                                    }
                                    "description" => update.description = Some(value),
                                    _ => {
                                        self.error_banner = Some(TuiErrorView::from(
                                            AppError::InvalidInput(format!(
                                                "Unknown field: {field}"
                                            )),
                                        ));
                                        return;
                                    }
                                }
                                match store.update(id, update) {
                                    Ok(topic) => {
                                        self.messages.push(TuiMessage::status(format!(
                                            "Updated topic '{}'",
                                            topic.name
                                        )));
                                    }
                                    Err(e) => {
                                        self.error_banner = Some(TuiErrorView::from(e));
                                    }
                                }
                            }
                            Err(e) => self.error_banner = Some(TuiErrorView::from(
                                AppError::StorageError(format!("Lock error: {}", e)),
                            )),
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else if args.len() >= 2 && args[1] == "delete" {
                    match self.gateway.topic_store() {
                        Ok(store_arc) => match store_arc.lock() {
                            Ok(store) => match store.delete(id) {
                                Ok(true) => {
                                    self.messages.push(TuiMessage::status(format!(
                                        "Deleted topic: {id}"
                                    )));
                                }
                                Ok(false) => {
                                    self.error_banner = Some(TuiErrorView::from(
                                        AppError::ConversationNotFound(format!(
                                            "Topic not found: {id}"
                                        )),
                                    ));
                                }
                                Err(e) => {
                                    self.error_banner = Some(TuiErrorView::from(e));
                                }
                            },
                            Err(e) => self.error_banner = Some(TuiErrorView::from(
                                AppError::StorageError(format!("Lock error: {}", e)),
                            )),
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else {
                    // View topic details
                    match self.gateway.topic_store() {
                        Ok(store_arc) => match store_arc.lock() {
                            Ok(store) => match store.get(id) {
                                Ok(Some(topic)) => {
                                    let status = serde_json::to_string(&topic.status)
                                        .unwrap_or_default()
                                        .trim_matches('"')
                                        .to_string();
                                    let mut lines = vec![
                                        format!("Topic: {}", topic.name),
                                        format!("Status: {}", status),
                                        format!("Progress: {}%", topic.progress),
                                    ];
                                    if !topic.description.is_empty() {
                                        lines.push(format!(
                                            "Description: {}",
                                            topic.description
                                        ));
                                    }
                                    if !topic.scope_in.is_empty() {
                                        lines.push("Scope-in:".to_string());
                                        for (i, item) in topic.scope_in.iter().enumerate() {
                                            lines.push(format!("  {}. {}", i + 1, item.goal));
                                        }
                                    }
                                    if let Some(ref extra) = topic.extra {
                                        lines.push(format!(
                                            "Extra: {}",
                                            serde_json::to_string_pretty(extra)
                                                .unwrap_or_default()
                                        ));
                                    }
                                    self.messages
                                        .push(TuiMessage::status(lines.join("\n")));
                                }
                                Ok(None) => {
                                    self.error_banner = Some(TuiErrorView::from(
                                        AppError::ConversationNotFound(format!(
                                            "Topic not found: {id}"
                                        )),
                                    ));
                                }
                                Err(e) => {
                                    self.error_banner = Some(TuiErrorView::from(e));
                                }
                            },
                            Err(e) => self.error_banner = Some(TuiErrorView::from(
                                AppError::StorageError(format!("Lock error: {}", e)),
                            )),
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                }
            }
        }
    }

    /// Handle `/neuron <args>` commands.
    fn handle_neuron_action(&mut self, args: Vec<String>) {
        if args.is_empty() {
            self.messages.push(TuiMessage::status(
                concat!(
                    "Neuron commands:\n",
                    "  /neuron list                          - List all neurons\n",
                    "  /neuron new <desc> [content]          - Create a new neuron\n",
                    "  /neuron <id>                          - View neuron details\n",
                    "  /neuron <id> set <field> <val>        - Update a field (desc/content/weight)\n",
                    "  /neuron <id> delete                   - Delete a neuron\n",
                    "  /neuron <id> connect <target> [weight]- Create/update a connection\n",
                    "  /neuron <id> disconnect <target>      - Remove a connection\n",
                    "  /neuron network <id> [depth]          - BFS network traversal"
                )
                .to_string(),
            ));
            return;
        }

        let action = args[0].as_str();
        let store_arc = match self.gateway.neuron_store() {
            Ok(s) => s,
            Err(e) => {
                self.error_banner = Some(TuiErrorView::from(e));
                return;
            }
        };

        match action {
            "list" => {
                match store_arc.lock() {
                    Ok(store) => match store.list_neurons() {
                        Ok(neurons) => {
                            if neurons.is_empty() {
                                self.messages
                                    .push(TuiMessage::status("No neurons found.".into()));
                                return;
                            }
                            let mut lines = vec!["Neurons:".to_string()];
                            for n in &neurons {
                                lines.push(format!(
                                    "  [w:{:+.1}] {} (id: {})",
                                    n.weight, n.desc, n.id
                                ));
                            }
                            self.messages.push(TuiMessage::status(lines.join("\n")));
                        }
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    },
                    Err(e) => self.error_banner = Some(TuiErrorView::from(
                        AppError::StorageError(format!("Lock error: {}", e)),
                    )),
                }
            }
            "new" if args.len() >= 2 => {
                let desc = args[1].clone();
                let content = if args.len() >= 3 { args[2..].join(" ") } else { String::new() };
                match store_arc.lock() {
                    Ok(store) => match store.create_neuron(&desc, &content, 0.0) {
                        Ok(n) => {
                            self.messages.push(TuiMessage::status(format!(
                                "Created neuron '{}' (id: {})",
                                n.desc, n.id
                            )));
                        }
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    },
                    Err(e) => self.error_banner = Some(TuiErrorView::from(
                        AppError::StorageError(format!("Lock error: {}", e)),
                    )),
                }
            }
            "network" if args.len() >= 2 => {
                let id = args[1].clone();
                let depth = if args.len() >= 3 {
                    args[2].parse::<usize>().unwrap_or(3)
                } else {
                    3
                };
                match store_arc.lock() {
                    Ok(store) => match store.get_network(&id, depth) {
                        Ok(network) => {
                            if network.is_empty() {
                                self.messages.push(TuiMessage::status(format!(
                                    "No network found for neuron: {id}"
                                )));
                                return;
                            }
                            let mut lines = vec![format!("Network (depth={depth}):")];
                            for n in &network {
                                lines.push(format!(
                                    "  [w:{:+.1}] {} (id: {})",
                                    n.weight, n.desc, n.id
                                ));
                            }
                            self.messages.push(TuiMessage::status(lines.join("\n")));
                        }
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    },
                    Err(e) => self.error_banner = Some(TuiErrorView::from(
                        AppError::StorageError(format!("Lock error: {}", e)),
                    )),
                }
            }
            id => {
                // View / set / delete / connect / disconnect
                if args.len() >= 3 && args[1] == "set" && args.len() >= 4 {
                    let field = args[2].as_str();
                    let value = args[3..].join(" ");
                    match store_arc.lock() {
                        Ok(store) => {
                            let mut update = crate::core::NeuronUpdate::default();
                            match field {
                                "desc" => update.desc = Some(value),
                                "content" => update.content = Some(value),
                                "weight" => {
                                    if let Ok(w) = value.parse::<f64>() {
                                        update.weight = Some(w);
                                    } else {
                                        self.error_banner = Some(TuiErrorView::from(
                                            AppError::InvalidInput("Invalid weight".into()),
                                        ));
                                        return;
                                    }
                                }
                                _ => {
                                    self.error_banner = Some(TuiErrorView::from(
                                        AppError::InvalidInput(format!(
                                            "Unknown field: {field}"
                                        )),
                                    ));
                                    return;
                                }
                            }
                            match store.update_neuron(id, update) {
                                Ok(n) => {
                                    self.messages.push(TuiMessage::status(format!(
                                        "Updated neuron '{}'",
                                        n.desc
                                    )));
                                }
                                Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                            }
                        }
                        Err(e) => self.error_banner = Some(TuiErrorView::from(
                            AppError::StorageError(format!("Lock error: {}", e)),
                        )),
                    }
                } else if args.len() >= 2 && args[1] == "delete" {
                    match store_arc.lock() {
                        Ok(store) => match store.delete_neuron(id) {
                            Ok(true) => {
                                self.messages.push(TuiMessage::status(format!(
                                    "Deleted neuron: {id}"
                                )));
                            }
                            Ok(false) => {
                                self.error_banner = Some(TuiErrorView::from(
                                    AppError::ConversationNotFound(format!(
                                        "Neuron not found: {id}"
                                    )),
                                ));
                            }
                            Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(
                            AppError::StorageError(format!("Lock error: {}", e)),
                        )),
                    }
                } else if args.len() >= 3 && args[1] == "connect" {
                    let target = args[2].clone();
                    let weight = if args.len() >= 4 {
                        args[3].parse::<f64>().unwrap_or(1.0)
                    } else {
                        1.0
                    };
                    match store_arc.lock() {
                        Ok(store) => match store.link(id, &target, weight) {
                            Ok(_) => {
                                self.messages.push(TuiMessage::status(format!(
                                    "Linked {} --[{}]--> {}",
                                    id, weight, target
                                )));
                            }
                            Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(
                            AppError::StorageError(format!("Lock error: {}", e)),
                        )),
                    }
                } else if args.len() >= 3 && args[1] == "disconnect" {
                    let target = args[2].clone();
                    match store_arc.lock() {
                        Ok(store) => match store.unlink(id, &target) {
                            Ok(true) => {
                                self.messages.push(TuiMessage::status(format!(
                                    "Removed link {} -> {}",
                                    id, target
                                )));
                            }
                            Ok(false) => {
                                self.error_banner = Some(TuiErrorView::from(
                                    AppError::ConversationNotFound(format!(
                                        "Link not found: {id} -> {target}"
                                    )),
                                ));
                            }
                            Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(
                            AppError::StorageError(format!("Lock error: {}", e)),
                        )),
                    }
                } else {
                    // View neuron details
                    match store_arc.lock() {
                        Ok(store) => {
                            match store.get_neuron(id) {
                                Ok(Some(n)) => {
                                    let conns = store.get_connections(id).unwrap_or_default();
                                    let mut lines = vec![
                                        format!("Neuron: {} (id: {})", n.desc, n.id),
                                        format!("Content: {}", n.content),
                                        format!("Weight: {}", n.weight),
                                    ];
                                    if !conns.is_empty() {
                                        lines.push("Connections:".into());
                                        for c in &conns {
                                            lines.push(format!(
                                                "  {} --[{}]--> {}",
                                                c.source, c.weight, c.target
                                            ));
                                        }
                                    }
                                    self.messages.push(TuiMessage::status(lines.join("\n")));
                                }
                                Ok(None) => {
                                    self.error_banner = Some(TuiErrorView::from(
                                        AppError::ConversationNotFound(format!(
                                            "Neuron not found: {id}"
                                        )),
                                    ));
                                }
                                Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                            }
                        }
                        Err(e) => self.error_banner = Some(TuiErrorView::from(
                            AppError::StorageError(format!("Lock error: {}", e)),
                        )),
                    }
                }
            }
        }
    }

    fn switch_session(&mut self, session_id: String) {
        let sid = session_id.clone();
        self.active_session_id = session_id.clone();
        self.messages.clear();

        let history = self
            .gateway
            .history(Some(session_id))
            .unwrap_or_default();
        for (i, msg) in history.iter().enumerate() {
            let role = match msg.role {
                MessageRole::User => TuiMessageRole::User,
                MessageRole::Assistant => TuiMessageRole::Assistant,
                MessageRole::System => TuiMessageRole::Status,
                MessageRole::Compaction => TuiMessageRole::Status,
            };
            self.messages.push(TuiMessage {
                id: format!("{}-{i}", self.active_session_id),
                role,
                content: msg.content.clone(),
                timestamp: Some(msg.timestamp),
                collapsed: false,
            });
        }

        self.focus = FocusPane::Input;
        self.show_sessions_list = false;
        self.error_banner = None;
        self.scroll_to_bottom();

        // Show a status message indicating the switch
        let short_id = if sid.len() > 16 {
            format!("{}..{}", &sid[..8], &sid[sid.len() - 4..])
        } else {
            sid
        };
        let msg_count = self.messages.len();
        self.messages.push(TuiMessage::status(format!(
            "Switched to session {short_id} ({msg_count} messages)"
        )));
    }

    /// Select the currently highlighted session from the sessions list.
    fn select_current_session(&mut self) {
        if let Some(idx) = self.session_list_state.selected() {
            if idx < self.conversations.len() {
                let session_id = self.conversations[idx].id.clone();
                self.switch_session(session_id);
            } else if idx == self.conversations.len() {
                let _ = self.create_new_session(ConversationMode::Chat);
            } else if idx == self.conversations.len() + 1 {
                let _ = self.create_new_session(ConversationMode::Agent);
            }
        }
    }

    /// Create a new session with the given mode and switch to it.
    fn create_new_session(&mut self, mode: ConversationMode) -> AppResult<()> {
        let new_id = self.gateway.create_new_conversation(mode)?;
        self.switch_session(new_id);
        self.refresh_conversations()?;
        Ok(())
    }

    /// Refresh the cached conversations list from the gateway.
    fn refresh_conversations(&mut self) -> AppResult<()> {
        self.conversations = self.gateway.list_conversations()?;
        self.status = self.gateway.status()?;
        Ok(())
    }

    /// Reload chat messages from the gateway for the active session.
    fn reload_messages(&mut self) {
        let history = self
            .gateway
            .history(Some(self.active_session_id.clone()))
            .unwrap_or_default();
        self.messages.clear();
        for (i, msg) in history.iter().enumerate() {
            let role = match msg.role {
                MessageRole::User => TuiMessageRole::User,
                MessageRole::Assistant => TuiMessageRole::Assistant,
                MessageRole::System => TuiMessageRole::Status,
                MessageRole::Compaction => TuiMessageRole::Status,
            };
            self.messages.push(TuiMessage {
                id: format!("{}-{i}", self.active_session_id),
                role,
                content: msg.content.clone(),
                timestamp: Some(msg.timestamp),
                collapsed: false,
            });
        }
        self.scroll_to_bottom();
    }

    /// Scroll to the bottom of the chat area (newest messages).
    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    fn next_task_id(&mut self) -> u64 {
        self.task_counter += 1;
        self.task_counter
    }

    /// Update command suggestions based on current input text.
    fn update_suggestions(&mut self) {
        let text = self
            .input
            .lines()
            .first()
            .map_or(String::new(), |v| v.trim().to_string());
        if text.starts_with('/') && text.len() >= 1 {
            let filter = text[1..].to_lowercase();
            let all = super::commands::cmd_help_text();
            let matched: Vec<(String, String)> = all
                .into_iter()
                .filter(|(cmd, _)| cmd.to_lowercase().contains(&filter))
                .collect();

            if matched.is_empty() {
                self.show_suggestions = false;
                self.suggestions.clear();
            } else {
                self.show_suggestions = true;
                self.suggestions = matched;
                self.suggestion_index = self.suggestion_index.min(self.suggestions.len() - 1);
            }
        } else {
            self.show_suggestions = false;
            self.suggestions.clear();
        }
    }

    /// Fill the input with the currently highlighted suggestion.
    fn fill_suggestion(&mut self) {
        if self.suggestion_index >= self.suggestions.len() {
            return;
        }
        let (cmd, _) = &self.suggestions[self.suggestion_index];
        // Get the actual command name (before the first space or paren)
        let cmd_name = cmd.split_whitespace().next().unwrap_or(cmd);
        // Replace the input with the command + trailing space
        let mut textarea = TextArea::default();
        textarea.insert_str(format!("{cmd_name} "));
        self.input = textarea;
        self.show_suggestions = false;
        self.suggestions.clear();
    }
}

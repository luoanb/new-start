use ratatui::widgets::ListState;
use ratatui_textarea::TextArea;

use crate::core::{
    AppError, AppResult, CandidateQuery, ChatModelSelection, ChatOptions, Conversation,
    ConversationMode, CreateNeuronInput, EnsureSystemOpts, Gateway, MessageRole,
    ModelAppendTemplate, ModelCallInput, ModelInfo, NeuronUpdate, ProviderInfo, RuntimeStatus,
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
        let history = gateway
            .history(Some(active_session_id.clone()))
            .unwrap_or_default();
        let mut messages = Vec::new();
        for (i, msg) in history.iter().enumerate() {
            let role = match msg.role {
                MessageRole::User => TuiMessageRole::User,
                MessageRole::Assistant => TuiMessageRole::Assistant,
                MessageRole::System => TuiMessageRole::Status,
                MessageRole::Tool => TuiMessageRole::Status,
                MessageRole::Compaction => TuiMessageRole::Status,
            };
            messages.push(TuiMessage {
                id: format!("{}-{i}", active_session_id),
                role,
                content: msg.text().to_string(),
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
        app.messages
            .push(TuiMessage::status("Agent App TUI started.".to_string()));
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
            TuiAction::NewSession => match self.create_new_session(ConversationMode::Chat) {
                Ok(()) => {}
                Err(error) => {
                    self.error_banner = Some(TuiErrorView::from(error));
                }
            },
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
                        self.session_list_state.select(Some((i + len - 1) % len));
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
                    let len = self.conversations.len() + 3;
                    if len > 0 {
                        self.session_list_state.select(Some((i + 1) % len));
                    }
                }
            }
            TuiAction::ListPrev => {
                if self.show_sessions_list {
                    let len = self.conversations.len() + 3;
                    if len > 0 {
                        let i = self.session_list_state.selected().unwrap_or(0);
                        self.session_list_state.select(Some((i + len - 1) % len));
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
            Command::NewSystem => {
                if let Err(error) = self.create_new_session(ConversationMode::System) {
                    self.error_banner = Some(TuiErrorView::from(error));
                }
            }
            Command::NewAssistant => {
                if let Err(error) = self.create_new_session(ConversationMode::Assistant) {
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
                    self.messages
                        .push(TuiMessage::status(format!("Skills:\n{text}")));
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
            Command::History => match self.gateway.history(Some(self.active_session_id.clone())) {
                Ok(history) => {
                    if history.is_empty() {
                        self.messages.push(TuiMessage::status("No history.".into()));
                    } else {
                        let lines: Vec<String> = history
                            .iter()
                            .map(|msg| {
                                let role = match msg.role {
                                    MessageRole::User => "user",
                                    MessageRole::Assistant => "assistant",
                                    MessageRole::System => "system",
                                    MessageRole::Tool => "tool",
                                    MessageRole::Compaction => "compaction",
                                };
                                format!("  [{role}] {}", msg.text())
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
            },
            Command::Clear => {
                match self
                    .gateway
                    .clear_conversation(Some(self.active_session_id.clone()))
                {
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
            Command::Status => match self.gateway.status() {
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
            },
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
                self.tasks.push(TuiTaskBlock::new(
                    task_id.clone(),
                    TuiTaskKind::ModelCall,
                    label,
                ));

                match self
                    .gateway
                    .call_model(crate::core::ModelCallRequest {
                        provider_id,
                        model_id,
                        messages: ModelCallInput::assemble(
                            &[],
                            "",
                            "",
                            &message,
                            ModelAppendTemplate::Neuron,
                        )
                        .messages,
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
            Command::TopicAction(args) => {
                self.handle_topic_action(args);
            }
            Command::NeuronAction(args) => {
                self.handle_neuron_action(args).await;
            }
            Command::PollAction(args) => {
                self.handle_poll_command(args).await;
            }
            Command::Close(session_id) => match self.gateway.session_tracker().close(&session_id) {
                Ok(msg) => {
                    self.messages.push(TuiMessage::status(msg));
                }
                Err(e) => {
                    self.error_banner = Some(TuiErrorView::from(e));
                }
            },
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
        self.tasks.push(TuiTaskBlock::new(
            task_id.clone(),
            TuiTaskKind::ModelCall,
            label,
        ));

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
                if response.conversation_id != self.active_session_id {
                    self.switch_session(response.conversation_id.clone());
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
                    self.error_banner = Some(TuiErrorView::from(AppError::StorageError(format!(
                        "Lock error: {}",
                        e
                    ))));
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
                    "  /topic <id> set <f> <v>     - Update name/description\n",
                    "  /topic <id> scope-add <goal> --done <contract>\n",
                    "  /topic <id> scope-delete <item_id>\n",
                    "  /topic <id> scope-complete <item_id>\n",
                    "  /topic <id> pause|resume\n",
                    "  /topic <id> delete          - Delete a topic\n",
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
                        Err(e) => {
                            self.error_banner = Some(TuiErrorView::from(AppError::StorageError(
                                format!("Lock error: {}", e),
                            )))
                        }
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
                                    "description" => update.description = Some(value),
                                    _ => {
                                        self.error_banner =
                                            Some(TuiErrorView::from(AppError::InvalidInput(
                                                format!("Unknown field: {field}"),
                                            )));
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
                            Err(e) => {
                                self.error_banner = Some(TuiErrorView::from(
                                    AppError::StorageError(format!("Lock error: {}", e)),
                                ))
                            }
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else if args.len() >= 3 && args[1] == "scope-add" {
                    let Some(done_index) = args.iter().position(|arg| arg == "--done") else {
                        self.error_banner = Some(TuiErrorView::from(AppError::InvalidInput(
                            "scope-add requires --done <contract>".into(),
                        )));
                        return;
                    };
                    let goal = args[2..done_index].join(" ");
                    let done_contract = args[done_index + 1..].join(" ");
                    match self.gateway.topic_store() {
                        Ok(store_arc) => match store_arc.lock() {
                            Ok(store) => match store.add_scope_item(id, &goal, &done_contract) {
                                Ok(topic) => self.messages.push(TuiMessage::status(format!(
                                    "Added scope item to '{}' ({}%)",
                                    topic.name, topic.progress
                                ))),
                                Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                            },
                            Err(e) => {
                                self.error_banner = Some(TuiErrorView::from(
                                    AppError::StorageError(format!("Lock error: {}", e)),
                                ))
                            }
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else if args.len() >= 3 && args[1] == "scope-delete" {
                    match self.gateway.topic_store() {
                        Ok(store_arc) => match store_arc.lock() {
                            Ok(store) => match store.delete_scope_item(id, &args[2]) {
                                Ok(topic) => self.messages.push(TuiMessage::status(format!(
                                    "Deleted scope item from '{}' ({}%)",
                                    topic.name, topic.progress
                                ))),
                                Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                            },
                            Err(e) => {
                                self.error_banner = Some(TuiErrorView::from(
                                    AppError::StorageError(format!("Lock error: {}", e)),
                                ))
                            }
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else if args.len() >= 3 && args[1] == "scope-complete" {
                    match self.gateway.topic_store() {
                        Ok(store_arc) => match store_arc.lock() {
                            Ok(store) => match store.complete_scope_item(id, &args[2]) {
                                Ok(topic) => self.messages.push(TuiMessage::status(format!(
                                    "Completed scope item in '{}' ({}%)",
                                    topic.name, topic.progress
                                ))),
                                Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                            },
                            Err(e) => {
                                self.error_banner = Some(TuiErrorView::from(
                                    AppError::StorageError(format!("Lock error: {}", e)),
                                ))
                            }
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else if args.len() >= 2 && (args[1] == "pause" || args[1] == "resume") {
                    match self.gateway.topic_store() {
                        Ok(store_arc) => match store_arc.lock() {
                            Ok(store) => {
                                let result = if args[1] == "pause" {
                                    store.pause(id)
                                } else {
                                    store.resume(id)
                                };
                                match result {
                                    Ok(topic) => self.messages.push(TuiMessage::status(format!(
                                        "Topic '{}' is now {:?}",
                                        topic.name, topic.status
                                    ))),
                                    Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                                }
                            }
                            Err(e) => {
                                self.error_banner = Some(TuiErrorView::from(
                                    AppError::StorageError(format!("Lock error: {}", e)),
                                ))
                            }
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else if args.len() >= 2 && args[1] == "delete" {
                    match self.gateway.topic_store() {
                        Ok(store_arc) => match store_arc.lock() {
                            Ok(store) => match store.delete(id) {
                                Ok(true) => {
                                    self.messages
                                        .push(TuiMessage::status(format!("Deleted topic: {id}")));
                                }
                                Ok(false) => {
                                    self.error_banner =
                                        Some(TuiErrorView::from(AppError::ConversationNotFound(
                                            format!("Topic not found: {id}"),
                                        )));
                                }
                                Err(e) => {
                                    self.error_banner = Some(TuiErrorView::from(e));
                                }
                            },
                            Err(e) => {
                                self.error_banner = Some(TuiErrorView::from(
                                    AppError::StorageError(format!("Lock error: {}", e)),
                                ))
                            }
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
                                        lines.push(format!("Description: {}", topic.description));
                                    }
                                    if !topic.scope_in.is_empty() {
                                        lines.push("Scope-in:".to_string());
                                        for (i, item) in topic.scope_in.iter().enumerate() {
                                            lines.push(format!(
                                                "  {}. {} (id: {}, status: {})",
                                                i + 1,
                                                item.goal,
                                                item.id,
                                                item.status
                                            ));
                                            lines
                                                .push(format!("     Done: {}", item.done_contract));
                                        }
                                    }
                                    if let Some(ref extra) = topic.extra {
                                        lines.push(format!(
                                            "Extra: {}",
                                            serde_json::to_string_pretty(extra).unwrap_or_default()
                                        ));
                                    }
                                    self.messages.push(TuiMessage::status(lines.join("\n")));
                                }
                                Ok(None) => {
                                    self.error_banner =
                                        Some(TuiErrorView::from(AppError::ConversationNotFound(
                                            format!("Topic not found: {id}"),
                                        )));
                                }
                                Err(e) => {
                                    self.error_banner = Some(TuiErrorView::from(e));
                                }
                            },
                            Err(e) => {
                                self.error_banner = Some(TuiErrorView::from(
                                    AppError::StorageError(format!("Lock error: {}", e)),
                                ))
                            }
                        },
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                }
            }
        }
    }

    /// Handle `/poll` commands.
    async fn handle_poll_command(&mut self, args: Vec<String>) {
        let action = args.first().map(|s| s.as_str()).unwrap_or("status");
        match action {
            "status" | "" => match self.gateway.poll_status() {
                Ok(status) => {
                    self.messages.push(TuiMessage::status(format!(
                        "Poller: state={:?}, ticks={}, base_interval_ms={}, tasks={}, pending_trigger={}",
                        status.state,
                        status.tick_count,
                        status.base_interval_ms,
                        status.task_count,
                        status.pending_trigger
                    )));
                }
                Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
            },
            "pause" => match self.gateway.poll_pause() {
                Ok(()) => self
                    .messages
                    .push(TuiMessage::status("Poller paused".into())),
                Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
            },
            "resume" => match self.gateway.poll_resume() {
                Ok(()) => self
                    .messages
                    .push(TuiMessage::status("Poller resumed".into())),
                Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
            },
            "trigger" => match self.gateway.poll_trigger() {
                Ok(()) => self.messages.push(TuiMessage::status(
                    "Poller will fire all handlers on next tick".into(),
                )),
                Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
            },
            "step" => {
                let Some(ref model) = self.active_model.clone() else {
                    self.error_banner = Some(TuiErrorView::from(AppError::ModelNotSelected));
                    return;
                };
                match self
                    .gateway
                    .assistant_step(Some(self.active_session_id.clone()), model)
                    .await
                {
                    Ok(response) => {
                        self.messages
                            .push(TuiMessage::assistant(response.response, "poll-step".into()));
                    }
                    Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                }
            }
            _ => {
                self.messages.push(TuiMessage::status(
                    "Usage: /poll [status|pause|resume|trigger|step]".into(),
                ));
            }
        }
    }

    /// Handle `/neuron <args>` commands.
    async fn handle_neuron_action(&mut self, args: Vec<String>) {
        if args.is_empty() {
            self.messages.push(TuiMessage::status(
                concat!(
                    "Neuron commands:\n",
                    "  /neuron list                          - List all neurons\n",
                    "  /neuron new [--count N] <purpose>     - Create 1..=10 neurons via unified flow\n",
                    "  /neuron candidates <n> [--source-id <id>] [--min-new <n>]\n",
                    "  /neuron ensure-creator                - Ensure create_neuron system node\n",
                    "  /neuron bootstrap                     - Bootstrap create_neuron + assistant_select_neuron\n",
                    "  /neuron rebootstrap                   - Reset all known assistant_* prompts + bootstrap\n",
                    "  /neuron ensure-system <type>          - Ensure system prompt neuron\n",
                    "  /neuron reset-system <type>           - Reset system prompt (unlink edges, recreate)\n",
                    "  /neuron <id>                          - View neuron details\n",
                    "  /neuron <id> set <field> <val>        - Update desc/content\n",
                    "  /neuron <id> weight <delta>           - Add or subtract weight\n",
                    "  /neuron <id> tools <id,...>            - Set allowed tool IDs\n",
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
        let manager = self.gateway.neuron_manager();
        let store_arc = match self.gateway.neuron_store() {
            Ok(s) => s,
            Err(e) => {
                self.error_banner = Some(TuiErrorView::from(e));
                return;
            }
        };

        match action {
            "candidates" if args.len() >= 2 => {
                let n = match args[1].parse::<usize>() {
                    Ok(n) => n,
                    Err(_) => {
                        self.error_banner = Some(TuiErrorView::from(AppError::InvalidInput(
                            "candidates requires an integer n".into(),
                        )));
                        return;
                    }
                };
                let mut source_id = None;
                let mut min_new = 0usize;
                let mut index = 2;
                while index < args.len() {
                    match args[index].as_str() {
                        "--source-id" if index + 1 < args.len() => {
                            source_id = Some(args[index + 1].clone());
                            index += 2;
                        }
                        "--min-new" if index + 1 < args.len() => {
                            match args[index + 1].parse::<usize>() {
                                Ok(value) => min_new = value,
                                Err(_) => {
                                    self.error_banner =
                                        Some(TuiErrorView::from(AppError::InvalidInput(
                                            "--min-new requires an integer".into(),
                                        )));
                                    return;
                                }
                            }
                            index += 2;
                        }
                        unknown => {
                            self.error_banner = Some(TuiErrorView::from(AppError::InvalidInput(
                                format!("Unknown candidates argument: {unknown}"),
                            )));
                            return;
                        }
                    }
                }
                match manager
                    .select_candidates(CandidateQuery {
                        n,
                        source_id,
                        min_new,
                    })
                    .await
                {
                    Ok(neurons) => {
                        let lines = neurons
                            .iter()
                            .map(|neuron| {
                                format!(
                                    "  [w:{:+.1}] {} (id: {})",
                                    neuron.weight, neuron.desc, neuron.id
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        self.messages
                            .push(TuiMessage::status(format!("Neuron candidates:\n{lines}")));
                    }
                    Err(error) => self.error_banner = Some(TuiErrorView::from(error)),
                }
            }
            "ensure-creator" => match manager.ensure_creator() {
                Ok(neuron) => self.messages.push(TuiMessage::status(format!(
                    "Creator neuron ready: {} (id: {})",
                    neuron.desc, neuron.id
                ))),
                Err(error) => self.error_banner = Some(TuiErrorView::from(error)),
            },
            "bootstrap" => match manager.bootstrap().await {
                Ok(report) => self.messages.push(TuiMessage::status(format!(
                    "Bootstrap ready: create_neuron={}, assistant_select_neuron={}",
                    report.create_neuron_id, report.select_neuron_id
                ))),
                Err(error) => self.error_banner = Some(TuiErrorView::from(error)),
            },
            "rebootstrap" => match manager.rebootstrap().await {
                Ok(report) => self.messages.push(TuiMessage::status(format!(
                    "Rebootstrap ok: create_neuron={}, assistant_select_neuron={} \
                     (also reset match_topic/complete_scope/score_feedback)",
                    report.create_neuron_id, report.select_neuron_id
                ))),
                Err(error) => self.error_banner = Some(TuiErrorView::from(error)),
            },
            "ensure-system" if args.len() >= 2 => {
                match manager
                    .ensure_system_neuron(&args[1], EnsureSystemOpts { reset: false })
                    .await
                {
                    Ok(neuron) => self.messages.push(TuiMessage::status(format!(
                        "System neuron ready: type={} id={}",
                        args[1], neuron.id
                    ))),
                    Err(error) => self.error_banner = Some(TuiErrorView::from(error)),
                }
            }
            "reset-system" if args.len() >= 2 => {
                match manager
                    .ensure_system_neuron(&args[1], EnsureSystemOpts { reset: true })
                    .await
                {
                    Ok(neuron) => self.messages.push(TuiMessage::status(format!(
                        "System neuron reset: type={} id={}",
                        args[1], neuron.id
                    ))),
                    Err(error) => self.error_banner = Some(TuiErrorView::from(error)),
                }
            }
            "list" => match store_arc.lock() {
                Ok(store) => match store.list_neurons() {
                    Ok(neurons) => {
                        if neurons.is_empty() {
                            self.messages
                                .push(TuiMessage::status("No neurons found.".into()));
                            return;
                        }
                        let mut lines = vec!["Neurons:".to_string()];
                        for n in &neurons {
                            lines.push(format!("  [w:{:+.1}] {} (id: {})", n.weight, n.desc, n.id));
                        }
                        self.messages.push(TuiMessage::status(lines.join("\n")));
                    }
                    Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                },
                Err(e) => {
                    self.error_banner = Some(TuiErrorView::from(AppError::StorageError(format!(
                        "Lock error: {}",
                        e
                    ))))
                }
            },
            "new" if args.len() >= 2 => {
                let mut count = 1usize;
                let mut purpose_parts = Vec::new();
                let mut index = 1;
                while index < args.len() {
                    if args[index] == "--count" && index + 1 < args.len() {
                        match args[index + 1].parse::<usize>() {
                            Ok(value) => count = value,
                            Err(_) => {
                                self.error_banner =
                                    Some(TuiErrorView::from(AppError::InvalidInput(
                                        "--count requires an integer 1..=10".into(),
                                    )));
                                return;
                            }
                        }
                        index += 2;
                        continue;
                    }
                    purpose_parts.push(args[index].clone());
                    index += 1;
                }
                let purpose = purpose_parts.join(" ");
                if purpose.trim().is_empty() {
                    self.error_banner = Some(TuiErrorView::from(AppError::InvalidInput(
                        "new requires a purpose".into(),
                    )));
                    return;
                }
                match manager
                    .create_neuron(CreateNeuronInput::Purpose(purpose), None, count)
                    .await
                {
                    Ok(neurons) => {
                        let lines = neurons
                            .iter()
                            .map(|n| format!("  '{}' (id: {})", n.desc, n.id))
                            .collect::<Vec<_>>()
                            .join("\n");
                        self.messages.push(TuiMessage::status(format!(
                            "Created {} neuron(s):\n{lines}",
                            neurons.len()
                        )));
                    }
                    Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
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
                            if network.neurons.is_empty() {
                                self.messages.push(TuiMessage::status(format!(
                                    "No network found for neuron: {id}"
                                )));
                                return;
                            }
                            let mut lines = vec![format!(
                                "Network (depth={depth}): {} neurons, {} edges",
                                network.neurons.len(),
                                network.connections.len()
                            )];
                            for n in &network.neurons {
                                lines.push(format!(
                                    "  [w:{:+.1}] {} (id: {})",
                                    n.weight, n.desc, n.id
                                ));
                            }
                            if !network.connections.is_empty() {
                                lines.push("Edges:".into());
                                for c in &network.connections {
                                    lines.push(format!(
                                        "  {} --({:.2})--> {}",
                                        c.source, c.weight, c.target
                                    ));
                                }
                            }
                            self.messages.push(TuiMessage::status(lines.join("\n")));
                        }
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    },
                    Err(e) => {
                        self.error_banner = Some(TuiErrorView::from(AppError::StorageError(
                            format!("Lock error: {}", e),
                        )))
                    }
                }
            }
            id => {
                // View / set / delete / connect / disconnect
                if args.len() >= 3 && args[1] == "set" && args.len() >= 4 {
                    let field = args[2].as_str();
                    let value = args[3..].join(" ");
                    let update = match field {
                        "desc" => NeuronUpdate {
                            desc: Some(value),
                            content: None,
                            ..Default::default()
                        },
                        "content" => NeuronUpdate {
                            desc: None,
                            content: Some(value),
                            ..Default::default()
                        },
                        _ => {
                            self.error_banner = Some(TuiErrorView::from(AppError::InvalidInput(
                                format!("Unknown field: {field}; expected desc or content"),
                            )));
                            return;
                        }
                    };
                    match manager.update_content_for_admin(id, update) {
                        Ok(n) => self
                            .messages
                            .push(TuiMessage::status(format!("Updated neuron '{}'", n.desc))),
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else if args.len() >= 3 && args[1] == "weight" {
                    let delta = match args[2].parse::<f64>() {
                        Ok(delta) => delta,
                        Err(_) => {
                            self.error_banner = Some(TuiErrorView::from(AppError::InvalidInput(
                                "Invalid weight delta".into(),
                            )));
                            return;
                        }
                    };
                    match manager.adjust_weight(id, delta) {
                        Ok(n) => self.messages.push(TuiMessage::status(format!(
                            "Adjusted neuron '{}' weight to {}",
                            n.desc, n.weight
                        ))),
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else if args.len() >= 3 && args[1] == "system-type" {
                    self.error_banner = Some(TuiErrorView::from(AppError::InvalidInput(
                        "system-type is removed; use /neuron ensure-system <type> or reset-system"
                            .into(),
                    )));
                } else if args.len() >= 3 && args[1] == "tools" {
                    let tool_ids = args[2]
                        .split(',')
                        .filter(|value| !value.trim().is_empty())
                        .map(|value| value.trim().to_string())
                        .collect();
                    match manager.set_tool_ids_for_admin(id, tool_ids) {
                        Ok(n) => self.messages.push(TuiMessage::status(format!(
                            "Updated neuron '{}' tool IDs",
                            n.desc
                        ))),
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else if args.len() >= 2 && args[1] == "delete" {
                    match manager.delete_for_admin(id) {
                        Ok(true) => self
                            .messages
                            .push(TuiMessage::status(format!("Deleted neuron: {id}"))),
                        Ok(false) => {
                            self.error_banner =
                                Some(TuiErrorView::from(AppError::NeuronNotFound(id.to_string())));
                        }
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else if args.len() >= 3 && args[1] == "connect" {
                    let target = args[2].clone();
                    // Creation edge weight is always 0; use adjust-connection for deltas.
                    match manager.link_for_admin(id, &target, 0.0) {
                        Ok(conn) => self.messages.push(TuiMessage::status(format!(
                            "Linked {} --[{}]--> {}",
                            id, conn.weight, target
                        ))),
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else if args.len() >= 3 && args[1] == "disconnect" {
                    let target = args[2].clone();
                    match manager.unlink_for_admin(id, &target) {
                        Ok(true) => self.messages.push(TuiMessage::status(format!(
                            "Removed link {} -> {}",
                            id, target
                        ))),
                        Ok(false) => {
                            self.error_banner =
                                Some(TuiErrorView::from(AppError::ConversationNotFound(format!(
                                    "Link not found: {id} -> {target}"
                                ))));
                        }
                        Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                    }
                } else {
                    // View neuron details
                    match store_arc.lock() {
                        Ok(store) => match store.get_neuron(id) {
                            Ok(Some(n)) => {
                                let conns = store.get_connections(id).unwrap_or_default();
                                let mut lines = vec![
                                    format!("Neuron: {} (id: {})", n.desc, n.id),
                                    format!("Content: {}", n.content),
                                    format!("Weight: {}", n.weight),
                                    format!(
                                        "System type: {}",
                                        n.system_type.as_deref().unwrap_or("-")
                                    ),
                                    format!("Tool IDs: {}", n.tool_ids.join(", ")),
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
                                    AppError::NeuronNotFound(id.to_string()),
                                ));
                            }
                            Err(e) => self.error_banner = Some(TuiErrorView::from(e)),
                        },
                        Err(e) => {
                            self.error_banner = Some(TuiErrorView::from(AppError::StorageError(
                                format!("Lock error: {}", e),
                            )))
                        }
                    }
                }
            }
        }
    }

    fn switch_session(&mut self, session_id: String) {
        let sid = session_id.clone();
        self.active_session_id = session_id.clone();
        self.messages.clear();

        let history = self.gateway.history(Some(session_id)).unwrap_or_default();
        for (i, msg) in history.iter().enumerate() {
            let role = match msg.role {
                MessageRole::User => TuiMessageRole::User,
                MessageRole::Assistant => TuiMessageRole::Assistant,
                MessageRole::System => TuiMessageRole::Status,
                MessageRole::Tool => TuiMessageRole::Status,
                MessageRole::Compaction => TuiMessageRole::Status,
            };
            self.messages.push(TuiMessage {
                id: format!("{}-{i}", self.active_session_id),
                role,
                content: msg.text().to_string(),
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
                let _ = self.create_new_session(ConversationMode::System);
            } else if idx == self.conversations.len() + 2 {
                let _ = self.create_new_session(ConversationMode::Assistant);
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
                MessageRole::Tool => TuiMessageRole::Status,
                MessageRole::Compaction => TuiMessageRole::Status,
            };
            self.messages.push(TuiMessage {
                id: format!("{}-{i}", self.active_session_id),
                role,
                content: msg.text().to_string(),
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
        // Replace the input with the full command text + trailing space
        let mut textarea = TextArea::default();
        textarea.insert_str(format!("{cmd} "));
        self.input = textarea;
        self.show_suggestions = false;
        self.suggestions.clear();
    }
}

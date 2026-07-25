use agent_app_lib::core::{
    AppError, AppResult, ChatModelSelection, ChatOptions, Gateway, MessageRole, ModelCallRequest,
    ModelMessage, ModelMessageRole,
};
use std::io::{self, Write};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error [{}]: {}", error.code(), error);
        std::process::exit(error.exit_code());
    }
}

async fn run() -> AppResult<()> {
    let mut gateway = Gateway::default()?;
    let mut state = TuiState::load(&gateway)?;

    println!("Agent App TUI");
    println!("Type /help for commands, /exit to quit.");
    print_status(&gateway, &state)?;
    print_startup_hint(&state);

    loop {
        print_prompt(&gateway, &state)?;
        io::stdout().flush()?;

        let mut input = String::new();
        let bytes = io::stdin().read_line(&mut input)?;
        if bytes == 0 {
            println!();
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        match handle_input(&mut gateway, &mut state, input).await {
            Ok(TuiAction::Continue) => {}
            Ok(TuiAction::Exit) => break,
            Err(error) => print_tui_error(&error),
        }
    }

    Ok(())
}

#[derive(Debug, Default)]
struct TuiState {
    active_model: Option<ChatModelSelection>,
}

impl TuiState {
    fn load(gateway: &Gateway) -> AppResult<Self> {
        Ok(Self {
            active_model: gateway.default_model_selection()?,
        })
    }

    fn model_label(&self) -> String {
        self.active_model
            .as_ref()
            .map(|selection| format!("{}/{}", selection.provider_id, selection.model_id))
            .unwrap_or_else(|| "no-model".to_string())
    }
}

enum TuiAction {
    Continue,
    Exit,
}

async fn handle_input(
    gateway: &mut Gateway,
    state: &mut TuiState,
    input: &str,
) -> AppResult<TuiAction> {
    match input {
        "/help" => print_help(),
        "/skills" => print_skills(gateway),
        "/providers" => print_providers(gateway),
        "/sessions" => print_sessions(gateway)?,
        "/history" => print_history(gateway)?,
        "/clear" => {
            let cleared = gateway.clear_conversation(None)?;
            println!("cleared conversation: {cleared}");
            print_status(gateway, state)?;
        }
        "/status" => print_status(gateway, state)?,
        "/exit" | "/quit" => return Ok(TuiAction::Exit),
        command if command.starts_with("/models") => {
            let provider_id = command
                .split_whitespace()
                .nth(1)
                .map(std::string::ToString::to_string);
            print_models(gateway, provider_id)?;
        }
        command if command.starts_with("/use ") => {
            let selection = parse_use_command(command)?;
            gateway.require_model(&selection.provider_id, &selection.model_id)?;
            println!("selected> {}/{}", selection.provider_id, selection.model_id);
            state.active_model = Some(selection);
        }
        command if command.starts_with("/call ") => {
            let response = call_model_from_command(gateway, command).await?;
            println!("model> {}", response.output);
        }
        command if command.starts_with('/') => {
            return Err(AppError::InvalidInput(format!(
                "Unknown command: {command}. Type /help for commands."
            )));
        }
        message => {
            let selection = state
                .active_model
                .clone()
                .ok_or(AppError::ModelNotSelected)?;
            let response = gateway
                .send_model_message(
                    message,
                    ChatOptions {
                        provider_id: selection.provider_id,
                        model_id: selection.model_id,
                        conversation_id: None,
                    },
                )
                .await?;
            println!("assistant> {}", response.response);
        }
    }

    Ok(TuiAction::Continue)
}

fn print_help() {
    println!("Commands:");
    println!("  /help      Show help");
    println!("  /skills    List skills");
    println!("  /providers List model providers");
    println!("  /models    List models, optionally /models <provider>");
    println!("  /use       Select chat model: /use <provider> <model>");
    println!("  /call      Call a model: /call <provider> <model> <message>");
    println!("  /sessions  List conversations");
    println!("  /history   Show current conversation history");
    println!("  /clear     Clear current conversation");
    println!("  /status    Show runtime status");
    println!("  /exit      Quit");
}

fn print_providers(gateway: &Gateway) {
    for provider in gateway.list_providers() {
        println!(
            "provider> {} - {} ({})",
            provider.id, provider.display_name, provider.auth_env
        );
    }
}

fn print_models(gateway: &Gateway, provider_id: Option<String>) -> AppResult<()> {
    let models = gateway.list_models(provider_id)?;
    if models.is_empty() {
        println!("model> no configured models");
        println!("hint> Add providers.<id>.models to .agent-app/config.json.");
        return Ok(());
    }

    for model in models {
        println!(
            "model> {} provider={} chat={} tools={} streaming={}",
            model.id,
            model.provider_id,
            model.capabilities.chat,
            model.capabilities.tools,
            model.capabilities.streaming
        );
    }
    Ok(())
}

fn parse_use_command(command: &str) -> AppResult<ChatModelSelection> {
    let parts = command.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(AppError::InvalidInput(
            "Usage: /use <provider> <model>".into(),
        ));
    }

    Ok(ChatModelSelection {
        provider_id: parts[1].to_string(),
        model_id: parts[2].to_string(),
    })
}

async fn call_model_from_command(
    gateway: &Gateway,
    command: &str,
) -> AppResult<agent_app_lib::core::ModelCallResponse> {
    let parts = command.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 4 {
        return Err(AppError::InvalidInput(
            "Usage: /call <provider> <model> <message>".into(),
        ));
    }

    let provider_id = parts[1].to_string();
    let model_id = parts[2].to_string();
    let message = parts[3..].join(" ");

    gateway
        .call_model(ModelCallRequest {
            provider_id,
            model_id,
            messages: vec![ModelMessage {
                role: ModelMessageRole::User,
                content: message,
            }],
        })
        .await
}

fn print_status(gateway: &Gateway, state: &TuiState) -> AppResult<()> {
    let status = gateway.status()?;
    println!(
        "status> current={} model={} skills={} conversations={}",
        status.current_conversation_id,
        state.model_label(),
        status.skill_count,
        status.conversation_count
    );
    Ok(())
}

fn print_startup_hint(state: &TuiState) {
    if state.active_model.is_none() {
        println!("hint> Run /providers, /models <provider>, then /use <provider> <model>.");
    }
}

fn print_prompt(gateway: &Gateway, state: &TuiState) -> AppResult<()> {
    let status = gateway.status()?;
    print!(
        "agent-app [{}] {}> ",
        state.model_label(),
        status.current_conversation_id
    );
    Ok(())
}

fn print_tui_error(error: &AppError) {
    println!("error [{}]: {}", error.code(), error);
    match error {
        AppError::ModelNotSelected => {
            println!("hint> Use /use <provider> <model> before ordinary chat input.");
        }
        AppError::ProviderAuthMissing(_) => {
            println!("hint> Set the provider API key via environment variables or .agent-app/config.json.");
        }
        AppError::ModelNotFound(_) => {
            println!("hint> Run /models <provider> or update providers.<id>.models in .agent-app/config.json.");
        }
        AppError::LlmRequestFailed(_) => {
            println!("hint> Check the model name, API base URL, and provider credentials.");
        }
        AppError::InvalidInput(_) => {
            println!("hint> Type /help for commands.");
        }
        _ => {}
    }
}

fn print_skills(gateway: &Gateway) {
    for skill in gateway.list_skills() {
        println!("skill> {} - {}", skill.name, skill.description);
    }
}

fn print_sessions(gateway: &Gateway) -> AppResult<()> {
    for conversation in gateway.list_conversations()? {
        println!(
            "session> {} messages={} updated_at={}",
            conversation.id,
            conversation.messages.len(),
            conversation.updated_at
        );
    }
    Ok(())
}

fn print_history(gateway: &Gateway) -> AppResult<()> {
    for message in gateway.history(None)? {
        let role = match message.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
        };
        println!(
            "history> [{}] {}: {}",
            message.timestamp, role, message.content
        );
    }
    Ok(())
}

use agent_app_lib::core::{
    AppError, AppResult, Gateway, MessageRole, ModelCallRequest, ModelMessage, ModelMessageRole,
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

    println!("Agent App TUI");
    println!("Type /help for commands, /exit to quit.");
    print_status(&gateway)?;

    loop {
        print!("agent-app> ");
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

        match input {
            "/help" => print_help(),
            "/skills" => print_skills(&gateway),
            "/providers" => print_providers(&gateway),
            "/sessions" => print_sessions(&gateway)?,
            "/history" => print_history(&gateway)?,
            "/clear" => {
                let cleared = gateway.clear_conversation(None)?;
                println!("cleared conversation: {cleared}");
                print_status(&gateway)?;
            }
            "/status" => print_status(&gateway)?,
            "/exit" | "/quit" => break,
            command if command.starts_with("/models") => {
                let provider_id = command
                    .split_whitespace()
                    .nth(1)
                    .map(std::string::ToString::to_string);
                print_models(&gateway, provider_id)?;
            }
            command if command.starts_with("/call ") => {
                let response = call_model_from_command(&gateway, command).await?;
                println!("model> {}", response.output);
            }
            message => {
                let response = gateway.send_message(message, None)?;
                println!("assistant> {}", response.response);
            }
        }
    }

    Ok(())
}

fn print_help() {
    println!("Commands:");
    println!("  /help      Show help");
    println!("  /skills    List skills");
    println!("  /providers List model providers");
    println!("  /models    List models, optionally /models <provider>");
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
    for model in gateway.list_models(provider_id)? {
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

fn print_status(gateway: &Gateway) -> AppResult<()> {
    let status = gateway.status()?;
    println!(
        "status> current={} skills={} conversations={}",
        status.current_conversation_id, status.skill_count, status.conversation_count
    );
    Ok(())
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

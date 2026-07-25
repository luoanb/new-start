use agent_app_lib::core::{AppResult, Gateway, MessageRole};
use std::io::{self, Write};

fn main() {
    if let Err(error) = run() {
        eprintln!("error [{}]: {}", error.code(), error);
        std::process::exit(error.exit_code());
    }
}

fn run() -> AppResult<()> {
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
            "/sessions" => print_sessions(&gateway)?,
            "/history" => print_history(&gateway)?,
            "/clear" => {
                let cleared = gateway.clear_conversation(None)?;
                println!("cleared conversation: {cleared}");
                print_status(&gateway)?;
            }
            "/status" => print_status(&gateway)?,
            "/exit" | "/quit" => break,
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
    println!("  /sessions  List conversations");
    println!("  /history   Show current conversation history");
    println!("  /clear     Clear current conversation");
    println!("  /status    Show runtime status");
    println!("  /exit      Quit");
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

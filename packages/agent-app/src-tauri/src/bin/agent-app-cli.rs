use agent_app_lib::core::{
    AppError, AppResult, Conversation, Gateway, Message, MessageRole, RuntimeStatus, SkillInfo,
};
use std::{env, process};

fn main() {
    if let Err(error) = run() {
        eprintln!("error [{}]: {}", error.code(), error);
        process::exit(error.exit_code());
    }
}

fn run() -> AppResult<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let Some(command) = args.first().cloned() else {
        print_help();
        return Ok(());
    };
    args.remove(0);

    let mut gateway = Gateway::default()?;

    match command.as_str() {
        "chat" => {
            let (conversation_id, message_parts) = take_conversation_arg(args)?;
            let message = message_parts.join(" ");
            let response = gateway.send_message(message, conversation_id)?;
            println!("conversation: {}", response.conversation_id);
            println!("{}", response.response);
        }
        "skills" => print_skills(gateway.list_skills()),
        "sessions" => print_conversations(gateway.list_conversations()?),
        "history" => {
            let conversation_id = args.first().cloned();
            print_history(gateway.history(conversation_id)?);
        }
        "clear" => {
            let conversation_id = args.first().cloned();
            let cleared = gateway.clear_conversation(conversation_id)?;
            println!("cleared conversation: {cleared}");
        }
        "status" => print_status(gateway.status()?),
        "help" | "--help" | "-h" => print_help(),
        unknown => {
            return Err(AppError::InvalidInput(format!(
                "Unknown command: {unknown}. Run `agent-app-cli help`."
            )));
        }
    }

    Ok(())
}

fn take_conversation_arg(args: Vec<String>) -> AppResult<(Option<String>, Vec<String>)> {
    let mut conversation_id = None;
    let mut message_parts = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--conversation" | "-c" => {
                let Some(id) = args.get(index + 1) else {
                    return Err(AppError::InvalidInput(
                        "`--conversation` requires a conversation id".into(),
                    ));
                };
                conversation_id = Some(id.clone());
                index += 2;
            }
            value => {
                message_parts.push(value.to_string());
                index += 1;
            }
        }
    }

    if message_parts.is_empty() {
        return Err(AppError::InvalidInput(
            "`chat` requires a message, for example `agent-app-cli chat hello`".into(),
        ));
    }

    Ok((conversation_id, message_parts))
}

fn print_help() {
    println!("agent-app-cli");
    println!();
    println!("Commands:");
    println!("  chat <message> [--conversation <id>]  Send a message");
    println!("  skills                                List skills");
    println!("  sessions                              List conversations");
    println!("  history [conversation-id]             Show conversation history");
    println!("  clear [conversation-id]               Clear a conversation");
    println!("  status                                Show runtime status");
}

fn print_skills(skills: Vec<SkillInfo>) {
    for skill in skills {
        println!("{} - {}", skill.name, skill.description);
    }
}

fn print_conversations(conversations: Vec<Conversation>) {
    for conversation in conversations {
        println!(
            "{} | messages={} | updated_at={}",
            conversation.id,
            conversation.messages.len(),
            conversation.updated_at
        );
    }
}

fn print_history(messages: Vec<Message>) {
    for message in messages {
        let role = match message.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
        };
        println!("[{}] {}: {}", message.timestamp, role, message.content);
    }
}

fn print_status(status: RuntimeStatus) {
    println!("app: {}", status.app_name);
    println!("storage: {}", status.storage_path);
    println!("current_conversation: {}", status.current_conversation_id);
    println!("skills: {}", status.skill_count);
    println!("conversations: {}", status.conversation_count);
}

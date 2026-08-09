use pulsar_app_lib::core::{
    app_log, storage, AppError, AppResult, Conversation, Gateway, Message, MessageRole,
    ModelAppendTemplate, ModelCallInput, ModelCallRequest, ModelInfo, ProviderInfo, RuntimeStatus,
    SkillInfo,
};
use std::{env, process};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error [{}]: {}", error.code(), error);
        process::exit(error.exit_code());
    }
}

async fn run() -> AppResult<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let Some(command) = args.first().cloned() else {
        print_help();
        return Ok(());
    };
    args.remove(0);

    let storage_root = storage::default_root().unwrap_or_else(|_| ".".into());
    if let Err(error) = app_log::init(&storage_root, None, true) {
        eprintln!("warning: failed to init logging: {error}");
    }

    let gateway = Gateway::default()?;
    if let Err(error) = gateway.bootstrap_neurons().await {
        eprintln!(
            "warning: neuron bootstrap incomplete [{}]: {}",
            error.code(),
            error
        );
    }

    match command.as_str() {
        "chat" => {
            let (conversation_id, message_parts) = take_conversation_arg(args)?;
            let message = message_parts.join(" ");
            let response = gateway.send_message(message, conversation_id)?;
            println!("conversation: {}", response.conversation_id);
            println!("{}", response.response);
        }
        "skills" => print_skills(gateway.list_skills()),
        "providers" => print_providers(gateway.list_providers()),
        "models" => {
            let provider_id = args.first().cloned();
            print_models(gateway.list_models(provider_id)?);
        }
        "call-model" => {
            let (provider_id, model_id, message) = take_model_call_args(args)?;
            let response = gateway
                .call_model(ModelCallRequest {
                    provider_id,
                    model_id,
                    messages: ModelCallInput::assemble(
                        &[],
                        "",
                        "",
                        &message,
                        ModelAppendTemplate::Neuron,
                    ),
                    tools: None,
                })
                .await?;
            println!("provider: {}", response.provider_id);
            println!("model: {}", response.model_id);
            println!("{}", response.output);
        }
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
                "Unknown command: {unknown}. Run `pulsar-cli help`."
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
            "`chat` requires a message, for example `pulsar-cli chat hello`".into(),
        ));
    }

    Ok((conversation_id, message_parts))
}

fn take_model_call_args(args: Vec<String>) -> AppResult<(String, String, String)> {
    let mut provider_id = None;
    let mut model_id = None;
    let mut message_parts = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--provider" | "-p" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::InvalidInput(
                        "`--provider` requires a provider id".into(),
                    ));
                };
                provider_id = Some(value.clone());
                index += 2;
            }
            "--model" | "-m" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::InvalidInput(
                        "`--model` requires a model id".into(),
                    ));
                };
                model_id = Some(value.clone());
                index += 2;
            }
            value => {
                message_parts.push(value.to_string());
                index += 1;
            }
        }
    }

    let provider_id = provider_id
        .ok_or_else(|| AppError::InvalidInput("`call-model` requires `--provider <id>`".into()))?;
    let model_id = model_id
        .ok_or_else(|| AppError::InvalidInput("`call-model` requires `--model <id>`".into()))?;

    if message_parts.is_empty() {
        return Err(AppError::InvalidInput(
            "`call-model` requires a message".into(),
        ));
    }

    Ok((provider_id, model_id, message_parts.join(" ")))
}

fn print_help() {
    println!("pulsar-cli");
    println!();
    println!("Commands:");
    println!("  chat <message> [--conversation <id>]  Send a message");
    println!("  providers                             List model providers");
    println!("  models [provider-id]                  List models");
    println!("  call-model -p <id> -m <id> <message>  Call a model without session");
    println!("  skills                                List skills");
    println!("  sessions                              List conversations");
    println!("  history [conversation-id]             Show conversation history");
    println!("  clear [conversation-id]               Clear a conversation");
    println!("  status                                Show runtime status");
}

fn print_providers(providers: Vec<ProviderInfo>) {
    for provider in providers {
        println!(
            "{} | {} | auth={} | api_base={}",
            provider.id,
            provider.display_name,
            provider.auth_env,
            provider.api_base.unwrap_or_else(|| "default".to_string())
        );
    }
}

fn print_models(models: Vec<ModelInfo>) {
    for model in models {
        let ctx = model
            .context_window
            .map_or("-".to_string(), |v| format!("{v} ctx"));
        let price = match (model.pricing_input, model.pricing_output) {
            (Some(i), Some(o)) => format!("${i}/M in | ${o}/M out"),
            (Some(i), None) => format!("${i}/M in"),
            (None, Some(o)) => format!("${o}/M out"),
            (None, None) => String::new(),
        };
        let caps = {
            let mut parts = vec![];
            if model.capabilities.chat {
                parts.push("chat");
            }
            if model.capabilities.tools {
                parts.push("tools");
            }
            if model.capabilities.streaming {
                parts.push("streaming");
            }
            if model.capabilities.structured_output {
                parts.push("json");
            }
            parts.join(" ")
        };
        println!(
            "{} | {} | {} | {} | {}",
            model.id, model.provider_id, ctx, price, caps
        );
    }
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
            MessageRole::Tool => "tool",
            MessageRole::Compaction => "compaction",
        };
        println!("[{}] {}: {}", message.timestamp, role, message.text());
    }
}

fn print_status(status: RuntimeStatus) {
    println!("app: {}", status.app_name);
    println!("storage: {}", status.storage_path);
    println!("current_conversation: {}", status.current_conversation_id);
    println!("skills: {}", status.skill_count);
    println!("conversations: {}", status.conversation_count);
}

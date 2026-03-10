//! AI/GLM-5 CLI commands

use owo_colors::OwoColorize;
use std::io::{self, Write};
use tnbot_core::AiConfig;
use tnbot_core::ai::{DEFAULT_CONSTITUTION, Glm5Client, Message, PromptBuilder};
use tnbot_core::db::repository::{ConversationRepository, IdentityRepository, LibsqlRepository};

/// Send a one-shot prompt to GLM-5
pub async fn prompt(config: &AiConfig, text: String, json: bool) -> anyhow::Result<()> {
    let client = create_ai_client(config)?;

    let messages = vec![Message::system(DEFAULT_CONSTITUTION), Message::user(text)];

    match client.chat(messages).await {
        Ok(response) => {
            if json {
                let output = serde_json::json!({
                    "response": {
                        "content": response,
                        "model": config.model,
                    }
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!("{}", "Response:".green().bold());
                println!("{}", response);
            }
        }
        Err(e) => {
            eprintln!("{}", format!("Error: {}", e).red());
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Interactive chat session with GLM-5
pub async fn chat(config: &AiConfig) -> anyhow::Result<()> {
    let client = create_ai_client(config)?;
    let mut messages = vec![Message::system(DEFAULT_CONSTITUTION)];

    println!("{}", "GLM-5 Interactive Chat".green().bold());
    println!("{}", "Type 'exit' or 'quit' to end the session.".dimmed());
    println!();

    loop {
        print!("{} ", "You:".cyan().bold());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
            println!("{}", "Goodbye!".green());
            break;
        }

        if input.is_empty() {
            continue;
        }

        messages.push(Message::user(input.to_string()));

        match client.chat(messages.clone()).await {
            Ok(response) => {
                println!("{} {}", "GLM-5:".magenta().bold(), response);
                println!();
                messages.push(Message::assistant(response));
            }
            Err(e) => {
                eprintln!("{}", format!("Error: {}", e).red());
                messages.pop();
            }
        }
    }

    Ok(())
}

/// Build and display the prompt context for a thread
pub async fn context(
    _ai_config: &AiConfig, db_path: &std::path::Path, root_uri: String, json: bool,
) -> anyhow::Result<()> {
    use tnbot_core::db::DatabaseManager;

    let db_manager = DatabaseManager::open(db_path).await?;
    let conn = db_manager.db().connect()?;
    let repo = LibsqlRepository::new(conn);

    let thread = repo.get_thread_by_root(&root_uri).await?;

    if thread.is_empty() {
        eprintln!("{}", format!("No thread found for root URI: {}", root_uri).red());
        std::process::exit(1);
    }

    let mut identity_map = std::collections::HashMap::new();
    for msg in &thread {
        if !identity_map.contains_key(&msg.author_did) {
            match repo.get_by_did(&msg.author_did).await? {
                Some(identity) => {
                    identity_map.insert(msg.author_did.clone(), identity.handle);
                }
                None => {
                    identity_map.insert(msg.author_did.clone(), msg.author_did.clone());
                }
            }
        }
    }

    let prompt_builder = PromptBuilder::new(DEFAULT_CONSTITUTION);
    let resolve_handle = |did: &str| identity_map.get(did).map(|s| s.as_str()).unwrap_or(did).to_string();

    let messages = prompt_builder.build(&thread, resolve_handle);

    if json {
        let messages_json: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect();

        let output = serde_json::json!({
            "root_uri": root_uri,
            "message_count": messages.len(),
            "messages": messages_json,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Thread Context:".green().bold());
        println!("{} {}", "Root URI:".cyan(), root_uri);
        println!("{} {}", "Messages:".cyan(), messages.len());
        println!();

        for (i, msg) in messages.iter().enumerate() {
            match msg.role.as_str() {
                "system" => {
                    println!("{}. [{}] {}", i + 1, "SYSTEM".yellow(), "...".dimmed());
                }
                "user" => {
                    println!(
                        "{}. [{}] {}",
                        i + 1,
                        "USER".cyan(),
                        msg.content.as_deref().unwrap_or("(empty)").cyan()
                    );
                }
                "assistant" => {
                    println!(
                        "{}. [{}] {}",
                        i + 1,
                        "BOT".magenta(),
                        msg.content.as_deref().unwrap_or("(empty)").magenta()
                    );
                }
                _ => {
                    println!(
                        "{}. [{}] {}",
                        i + 1,
                        msg.role.to_uppercase(),
                        msg.content.as_deref().unwrap_or("(empty)")
                    );
                }
            }
        }
    }

    Ok(())
}

/// Simulate a response without posting
pub async fn simulate(
    ai_config: &AiConfig, db_path: &std::path::Path, root_uri: String, json: bool,
) -> anyhow::Result<()> {
    use tnbot_core::ai::ChatCompletionRequest;
    use tnbot_core::db::DatabaseManager;

    let client = create_ai_client(ai_config)?;
    let db_manager = DatabaseManager::open(db_path).await?;
    let conn = db_manager.db().connect()?;
    let repo = LibsqlRepository::new(conn);

    let thread = repo.get_thread_by_root(&root_uri).await?;

    if thread.is_empty() {
        eprintln!("{}", format!("No thread found for root URI: {}", root_uri).red());
        std::process::exit(1);
    }

    let mut identity_map = std::collections::HashMap::new();
    for msg in &thread {
        if !identity_map.contains_key(&msg.author_did) {
            match repo.get_by_did(&msg.author_did).await? {
                Some(identity) => {
                    identity_map.insert(msg.author_did.clone(), identity.handle);
                }
                None => {
                    identity_map.insert(msg.author_did.clone(), msg.author_did.clone());
                }
            }
        }
    }

    let prompt_builder = PromptBuilder::new(DEFAULT_CONSTITUTION);
    let resolve_handle = |did: &str| identity_map.get(did).map(|s| s.as_str()).unwrap_or(did).to_string();

    let messages = prompt_builder.build(&thread, resolve_handle);
    let request = ChatCompletionRequest::new(&ai_config.model, messages).with_max_tokens(ai_config.max_tokens);

    if json {
        let messages_json: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect();

        let output = serde_json::json!({
            "root_uri": root_uri,
            "model": ai_config.model,
            "messages": messages_json,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Simulation:".green().bold());
        println!("{} {}", "Root URI:".cyan(), root_uri);
        println!("{} {}", "Model:".cyan(), ai_config.model);
        println!("{} {}", "Messages:".cyan(), request.messages.len());
        println!();

        println!("{}", "Context:".yellow().bold());
        for (i, msg) in request.messages.iter().enumerate() {
            let (role_label, role_color_fn): (&str, fn(&str) -> String) = match msg.role.as_str() {
                "system" => ("SYSTEM", |s| s.yellow().to_string()),
                "user" => ("USER", |s| s.cyan().to_string()),
                "assistant" => ("BOT", |s| s.magenta().to_string()),
                _ => ("UNKNOWN", |s| s.to_string()),
            };
            println!(
                "{}. [{}] {}",
                i + 1,
                role_color_fn(role_label),
                msg.content.as_deref().unwrap_or("(empty)").dimmed()
            );
        }
        println!();
    }

    match client.chat_completion(request).await {
        Ok(response) => {
            if let Some(content) = response.content() {
                if json {
                    let output = serde_json::json!({
                        "response": {
                            "content": content,
                            "finish_reason": response.finish_reason().unwrap_or("unknown"),
                            "prompt_tokens": response.usage.prompt_tokens,
                            "completion_tokens": response.usage.completion_tokens,
                            "total_tokens": response.usage.total_tokens,
                        }
                    });
                    println!("{}", serde_json::to_string_pretty(&output)?);
                } else {
                    println!("{}", "Generated Response:".green().bold());
                    println!("{}", content);
                    println!();
                    println!(
                        "{} {} tokens (prompt: {}, completion: {})",
                        "Usage:".dimmed(),
                        response.usage.total_tokens,
                        response.usage.prompt_tokens,
                        response.usage.completion_tokens
                    );
                }
            } else {
                eprintln!("{}", "No content in response".red());
            }
        }
        Err(e) => {
            eprintln!("{}", format!("Error: {}", e).red());
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Create a GLM-5 client from configuration
fn create_ai_client(config: &AiConfig) -> anyhow::Result<Glm5Client> {
    if config.api_key.is_empty() {
        match Glm5Client::from_env() {
            Ok(client) => Ok(client),
            Err(_) => {
                eprintln!(
                    "{}",
                    "AI API key not configured. Set ai.api_key in config or TNBOT_AI__API_KEY / GLM_5_API_KEY environment variable.".red()
                );
                std::process::exit(1);
            }
        }
    } else {
        use tnbot_core::ai::Glm5Config;

        let glm_config = Glm5Config {
            api_key: config.api_key.clone(),
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            temperature: config.temperature,
            max_tokens: config.max_tokens,
        };

        Ok(Glm5Client::with_config(glm_config))
    }
}

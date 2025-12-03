use clap::Parser;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
const DEFAULT_MAX_TOKENS: u32 = 1024;
const DEFAULT_HISTORY_LINES: usize = 100;
const CONFIG_PATH: &str = ".config/llm-exec/config.json";
const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

const DEFAULT_SYSTEM_PROMPT: &str = r#"You are a command-line assistant that outputs ONLY shell commands.

RULES:
1. Output ONLY a single shell command - nothing else
2. NO explanations, NO markdown, NO code blocks, NO backticks, NO formatting
3. If you cannot help, output: echo "Error: <reason>"
4. Never suggest running "{}" - the user is already running that to talk to you

Your entire response must be a valid shell command that can be executed directly."#;

#[derive(Deserialize, Default)]
struct Config {
    /// Model to use
    model: Option<String>,
    /// Max tokens for response
    max_tokens: Option<u32>,
    /// Number of history lines to include
    history_lines: Option<usize>,
    /// Additional instructions to append to the system prompt
    system_prompt_suffix: Option<String>,
    /// Complete override of the system prompt (replaces default)
    system_prompt: Option<String>,
}

fn get_config_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(CONFIG_PATH))
}

fn load_config() -> Config {
    let Some(path) = get_config_path() else {
        return Config::default();
    };

    if !path.exists() {
        return Config::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
            eprintln!("Warning: Could not parse config file: {}", e);
            Config::default()
        }),
        Err(e) => {
            eprintln!("Warning: Could not read config file: {}", e);
            Config::default()
        }
    }
}

#[derive(Parser)]
#[command(name = "llm-exec")]
#[command(about = "Execute terminal commands based on LLM instructions")]
struct Args {
    /// The prompt describing what command you want to run
    prompt: Vec<String>,

    /// Number of history lines to include
    #[arg(short = 'n', long)]
    history_lines: Option<usize>,

    /// Skip confirmation and execute immediately
    #[arg(short = 'y', long)]
    yes: bool,

    /// Show what would be sent to the API without making a request
    #[arg(long)]
    dry_run: bool,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
}

fn get_history_file() -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    let history_files = [
        home.join(".zsh_history"),
        home.join(".bash_history"),
        home.join(".history"),
    ];

    history_files.into_iter().find(|f| f.exists())
}

fn get_shell_history(lines: usize) -> Result<String, Box<dyn std::error::Error>> {
    let history_file = get_history_file().ok_or("Could not find shell history file")?;

    let content = std::fs::read_to_string(&history_file)?;
    let history_lines: Vec<&str> = content.lines().collect();
    let start = history_lines.len().saturating_sub(lines);

    // Clean up zsh history format (removes timestamps like `: 1234567890:0;`)
    let cleaned: Vec<String> = history_lines[start..]
        .iter()
        .map(|line| {
            if line.starts_with(": ") && line.contains(";") {
                line.split_once(";")
                    .map(|(_, cmd)| cmd)
                    .unwrap_or(line)
                    .to_string()
            } else {
                line.to_string()
            }
        })
        .collect();

    Ok(cleaned.join("\n"))
}

fn append_to_history(command: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::OpenOptions;

    let history_file = get_history_file().ok_or("Could not find shell history file")?;
    let mut file = OpenOptions::new().append(true).open(&history_file)?;

    // Format depends on shell type
    let entry = if history_file.to_string_lossy().contains("zsh_history") {
        // zsh extended history format: `: timestamp:0;command`
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!(": {}:0;{}\n", timestamp, command)
    } else {
        // bash format: just the command
        format!("{}\n", command)
    };

    file.write_all(entry.as_bytes())?;
    Ok(())
}

async fn call_claude(prompt: &str, history: &str, config: &Config, argv0: &str) -> Result<String, Box<dyn std::error::Error>> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| "ANTHROPIC_API_KEY environment variable not set")?;

    let base_prompt = config
        .system_prompt
        .as_deref()
        .unwrap_or(DEFAULT_SYSTEM_PROMPT)
        .replace("{}", argv0);

    let system_prompt = if let Some(suffix) = &config.system_prompt_suffix {
        format!("{}\n\nThe user's recent shell history:\n{}\n\n{}", base_prompt, history, suffix)
    } else {
        format!("{}\n\nThe user's recent shell history:\n{}", base_prompt, history)
    };

    let model = config.model.as_deref().unwrap_or(DEFAULT_MODEL);
    let max_tokens = config.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);

    let request = AnthropicRequest {
        model: model.to_string(),
        max_tokens,
        system: system_prompt,
        messages: vec![Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        }],
    };

    let client = reqwest::Client::new();
    let response = client
        .post(API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", API_VERSION)
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        return Err(format!("API error ({}): {}", status, body).into());
    }

    let result: AnthropicResponse = response.json().await?;

    result
        .content
        .first()
        .and_then(|block| block.text.clone())
        .ok_or_else(|| "No response from Claude".into())
}

fn prompt_yes_no(prompt: &str) -> bool {
    print!("{} [y/N]: ", prompt);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

fn execute_command(command: &str) -> Result<(), Box<dyn std::error::Error>> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    let status = Command::new(&shell)
        .arg("-i")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        if let Some(code) = status.code() {
            std::process::exit(code);
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let prompt = if args.prompt.is_empty() {
        eprint!("What do you want to do? ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        input.trim().to_string()
    } else {
        args.prompt.join(" ")
    };

    if prompt.is_empty() {
        eprintln!("Error: No prompt provided");
        std::process::exit(1);
    }

    // Load config
    let config = load_config();

    // CLI overrides config, config overrides defaults
    let history_lines = args
        .history_lines
        .or(config.history_lines)
        .unwrap_or(DEFAULT_HISTORY_LINES);

    // Get shell history
    let history = match get_shell_history(history_lines) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Warning: Could not read shell history: {}", e);
            String::new()
        }
    };

    // Get argv[0] (the command name used to invoke this program)
    let argv0 = std::env::args()
        .next()
        .and_then(|p| {
            std::path::Path::new(&p)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "llm-exec".to_string());

    // Dry run mode - show what would be sent
    if args.dry_run {
        let model = config.model.as_deref().unwrap_or(DEFAULT_MODEL);
        let base_prompt = config.system_prompt.as_deref().unwrap_or(DEFAULT_SYSTEM_PROMPT).replace("{}", &argv0);
        let system_prompt = if let Some(suffix) = &config.system_prompt_suffix {
            format!("{}\n\nThe user's recent shell history:\n{}\n\n{}", base_prompt, history, suffix)
        } else {
            format!("{}\n\nThe user's recent shell history:\n{}", base_prompt, history)
        };

        println!("\x1b[1;36mModel:\x1b[0m {}", model);
        println!();
        println!("\x1b[1;36mSystem prompt:\x1b[0m");
        println!("{}", system_prompt);
        println!();
        println!("\x1b[1;36mUser prompt:\x1b[0m {}", prompt);
        return Ok(());
    }

    // Call Claude
    eprint!("Thinking...");
    let suggested_command = call_claude(&prompt, &history, &config, &argv0).await?;
    eprintln!("\r           \r"); // Clear "Thinking..."

    let suggested_command = suggested_command.trim();

    // Check if the response is an error sigil from the LLM
    if let Some(error_msg) = suggested_command
        .strip_prefix("echo \"Error: ")
        .and_then(|s| s.strip_suffix('"'))
    {
        eprintln!("\x1b[1;31mError:\x1b[0m {}", error_msg);
        std::process::exit(1);
    }

    // Present the command
    println!("\x1b[1;36mSuggested command:\x1b[0m");
    println!("\x1b[1;33m  {}\x1b[0m", suggested_command);
    println!();

    // Execute (with or without confirmation)
    let should_execute = args.yes || prompt_yes_no("Execute this command?");

    if should_execute {
        // Add to shell history before execution so it's available even if command fails
        if let Err(e) = append_to_history(&suggested_command) {
            eprintln!("Warning: Could not add to history: {}", e);
        }
        if !args.yes {
            println!();
        }
        execute_command(&suggested_command)?;
    } else {
        println!("Cancelled.");
    }

    Ok(())
}

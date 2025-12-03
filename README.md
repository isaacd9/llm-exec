# llm-exec

A CLI tool that translates natural language into shell commands using Claude AI. It reads your shell history for context and suggests appropriate commands.

## Installation

```bash
cargo install --git https://github.com/isaac/llm-exec
```

Or build from source:

```bash
cargo build --release
```

The binary will be at `./target/release/llm-exec`.

## Setup

Set your Anthropic API key:

```bash
export ANTHROPIC_API_KEY="your-api-key"
```

## Usage

```bash
llm-exec <prompt>
```

### Examples

```bash
llm-exec find all rust files modified today
llm-exec compress this directory into a tar.gz
llm-exec show disk usage sorted by size
```

### Options

- `-n, --history-lines <N>` - Number of shell history lines to include for context (default: 100)
- `-y, --yes` - Skip confirmation and execute immediately
- `--dry-run` - Show what would be sent to the API without making a request

```bash
llm-exec -n 50 "undo my last git commit"
llm-exec -y "list files"  # Execute without confirmation
```

## Configuration

Create a config file at `~/.config/llm-exec/config.json`:

```json
{
  "model": "claude-haiku-4-5-20251001",
  "max_tokens": 1024,
  "history_lines": 100,
  "system_prompt_suffix": "Additional instructions appended to the default prompt",
  "system_prompt": "Complete override of the system prompt"
}
```

All fields are optional:

- `model` - Claude model to use (default: `claude-haiku-4-5-20251001`)
- `max_tokens` - Maximum tokens for response (default: 1024)
- `history_lines` - Number of shell history lines to include (default: 100)
- `system_prompt_suffix` - Additional instructions appended to the default system prompt
- `system_prompt` - Complete replacement for the default system prompt

## How it works

1. Reads your recent shell history (~/.zsh_history, ~/.bash_history, or ~/.history)
2. Sends your prompt and history context to Claude
3. Displays the suggested command
4. Asks for confirmation before executing

## License

MIT

---

*This tool (and most of this README) was written with Claude. There may be bugs—PRs welcome!*

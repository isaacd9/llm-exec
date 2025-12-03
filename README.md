# llm-exec

A CLI tool that translates natural language into shell commands using Claude AI. It reads your shell history for context and suggests appropriate commands.

## Installation

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

```bash
llm-exec -n 50 "undo my last git commit"
```

## How it works

1. Reads your recent shell history (~/.zsh_history, ~/.bash_history, or ~/.history)
2. Sends your prompt and history context to Claude
3. Displays the suggested command
4. Asks for confirmation before executing

## License

MIT

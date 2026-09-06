# Term Code

Term Code is a Rust terminal UI for chatting with cloud AI providers.

## Features

- TUI chat interface
- First-run API key setup
- Persistent config at `$HOME/.nativestuff/Config.json`
- Provider model discovery
- Automatic model selection and fallback
- Streaming responses for OpenAI-compatible providers and Anthropic
- Gemini support
- `/model` and `/provider` selection
- Config errors and API errors shown in the TUI

## Providers

- OpenAI
- Anthropic
- Google Gemini
- DeepSeek
- Alibaba/Qwen
- OpenRouter
- Zhipu AI

## Build

```bash
cargo build --release
```

Run with:

```bash
cargo run --release
```

On first launch, select a provider, paste its API key, and enter the name Term Code should use for you.

## Commands

- `/model` — choose an available model
- `/provider` — change provider and API key
- `/config` — show the active provider and model
- `/new` or `/clear` — clear the conversation
- `/help` — show commands
- `/quit` or `/exit` — exit

`Config.json` is written with restrictive `0600` permissions on Unix systems.

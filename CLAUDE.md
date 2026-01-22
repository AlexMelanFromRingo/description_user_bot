# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Telegram userbot written in Rust that dynamically updates the user's profile description (bio) based on a configurable rotation schedule. Uses the `grammers` library for Telegram MTProto communication.

## Build Commands

```bash
# Check compilation without building
cargo check

# Build debug version
cargo build

# Build release version (optimized, smaller binary)
cargo build --release

# Run clippy lints
cargo clippy

# Run tests
cargo test

# Run specific test
cargo test test_name

# Run the main bot
cargo run --bin description_bot

# Run the validator tool
cargo run --bin validate_descriptions -- --help
```

## Architecture

### Module Structure

- **`src/config/`** - Configuration handling
  - `descriptions.rs` - Description JSON schema (`DescriptionConfig`), validation logic, character limits (70 free / 140 premium)
  - `settings.rs` - Telegram API config (`TelegramConfig`), bot settings (`BotSettings`), environment variable loading

- **`src/telegram/`** - Telegram client wrapper
  - `client.rs` - `TelegramBot` wraps grammers `Client` with bio update, authentication, and connection management
  - `rate_limiter.rs` - Rate limiting for API calls to avoid flood wait errors

- **`src/scheduler/`** - Description rotation logic
  - `state.rs` - `SchedulerState` tracks current description index, timing, pause state
  - `runner.rs` - `DescriptionScheduler` runs the rotation loop, listens for control messages

- **`src/commands/`** - Chat command handling
  - `types.rs` - `BotCommand` enum with parsing logic, `CommandResult` for responses
  - `handler.rs` - `CommandHandler` processes commands with `/description_bot` prefix

- **`src/validator/main.rs`** - Standalone CLI tool for validating description JSON files

### Key Dependencies

- `grammers-client` - Telegram MTProto client (from git, not crates.io)
- `grammers-session` - Session storage (SQLite)
- `tokio` - Async runtime
- `serde/serde_json` - JSON serialization
- `clap` - CLI argument parsing
- `tracing` - Logging

### Grammers API Notes

The grammers library uses a `SenderPool` architecture:
1. Create session: `SqliteSession::open(path).await`
2. Create pool: `SenderPool::new(session, api_id)` returns `{runner, updates, handle}`
3. Spawn runner: `tokio::spawn(runner.run())`
4. Create client: `Client::new(handle)`
5. Auth methods on `Client`: `is_authorized()`, `request_login_code()`, `sign_in()`, `check_password()`

Bio updates use raw API: `client.invoke(&tl::functions::account::UpdateProfile { about: Some(text), ... })`

## Configuration

### Environment Variables
- `TG_API_ID` - Telegram API ID (required)
- `TG_API_HASH` - Telegram API hash (required)
- `TG_SESSION_PATH` - Session file path (default: `session.db`)
- `DESCRIPTIONS_PATH` - Descriptions JSON path (default: `descriptions.json`)
- `COMMAND_PREFIX` - Bot command prefix (default: `/description_bot`)
- `MIN_UPDATE_INTERVAL` - Minimum seconds between bio updates (default: 60)

### Descriptions JSON Format
```json
{
  "descriptions": [
    {"id": "unique_id", "text": "Bio text here", "duration_secs": 3600}
  ],
  "is_premium": false
}
```

## Bot Commands

All commands use the `/description_bot` prefix:

### Control Commands
- `skip` - Skip to next description
- `status` / `s` - Show current status
- `goto <id>` - Jump to specific description
- `pause` / `resume` - Control rotation
- `reload` - Reload config file
- `set <text>` - Set temporary custom description
- `help` - Show help
- `info` - Show bot version info

### Description Management
- `list` / `ls` - List all descriptions
- `view <id>` / `v <id>` - View specific description details
- `add <id> <duration> <text>` / `a` - Add new description
- `edit <id> <new_text>` / `e` - Edit description text
- `duration <id> <seconds>` / `dur` - Change description duration
- `delete <id>` / `del` / `rm` - Delete description

### Text Validation
New descriptions are validated:
- No empty text allowed
- Character limit: 70 (free) / 140 (premium)
- Text-only content (no embedded objects)
- No invisible/zero-width characters

## Linting Configuration

Strict clippy lints enabled in `Cargo.toml`. Key allowed lints:
- `missing_errors_doc`, `missing_panics_doc` - Too verbose for this project
- `module_name_repetitions` - Common pattern in Rust
- `must_use_candidate` - Not all functions need `#[must_use]`

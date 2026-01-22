//! Command handler implementation.

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::types::{AddArgs, BotCommand, CommandResult, DurationArgs, EditArgs};
use crate::config::{Description, DescriptionConfig, MAX_BIO_LENGTH_FREE, MAX_BIO_LENGTH_PREMIUM};
use crate::scheduler::SchedulerState;

/// Handles bot commands and manages application state.
pub struct CommandHandler {
    /// Command prefix (e.g., "`/description_bot`").
    prefix: String,

    /// Shared scheduler state.
    scheduler_state: Arc<RwLock<SchedulerState>>,

    /// Description configuration.
    config: Arc<RwLock<DescriptionConfig>>,

    /// Path to the descriptions file (for saving changes).
    config_path: String,
}

impl CommandHandler {
    /// Creates a new command handler.
    #[must_use]
    pub fn new(
        prefix: String,
        scheduler_state: Arc<RwLock<SchedulerState>>,
        config: Arc<RwLock<DescriptionConfig>>,
        config_path: String,
    ) -> Self {
        Self {
            prefix,
            scheduler_state,
            config,
            config_path,
        }
    }

    /// Tries to parse and execute a command from a message.
    ///
    /// Returns `None` if the message is not a command.
    pub async fn try_handle(&self, message_text: &str) -> Option<CommandResult> {
        let command = BotCommand::parse(message_text, &self.prefix)?;

        debug!("Handling command: {}", command);
        let result = self.execute(command).await;
        info!(
            "Command result: success={}, trigger_update={}",
            result.success, result.trigger_update
        );

        Some(result)
    }

    /// Executes a parsed command.
    async fn execute(&self, command: BotCommand) -> CommandResult {
        match command {
            BotCommand::Skip => self.handle_skip().await,
            BotCommand::Status => self.handle_status().await,
            BotCommand::List => self.handle_list().await,
            BotCommand::View(id) => self.handle_view(&id).await,
            BotCommand::Goto(target) => self.handle_goto(&target).await,
            BotCommand::Pause => self.handle_pause().await,
            BotCommand::Resume => self.handle_resume().await,
            BotCommand::Reload => self.handle_reload().await,
            BotCommand::Help => self.handle_help(),
            BotCommand::Set(text) => self.handle_set(&text).await,
            BotCommand::Add(args) => self.handle_add(args).await,
            BotCommand::Edit(args) => self.handle_edit(args).await,
            BotCommand::Duration(args) => self.handle_duration(args).await,
            BotCommand::Delete(id) => self.handle_delete(&id).await,
            BotCommand::Info => self.handle_info(),
        }
    }

    async fn handle_skip(&self) -> CommandResult {
        let mut state = self.scheduler_state.write().await;

        if state.is_paused {
            return CommandResult::error("Cannot skip while paused. Use 'resume' first.");
        }

        state.skip_current = true;
        CommandResult::success_with_update("✓ Skipping current description...")
    }

    async fn handle_status(&self) -> CommandResult {
        let state = self.scheduler_state.read().await;
        let config = self.config.read().await;

        let current_desc = config.get(state.current_index).map_or_else(
            || "None".to_owned(),
            |d| format!("[{}] \"{}\"", d.id, truncate(&d.text, 30)),
        );

        let status = if state.is_paused {
            "⏸ Paused"
        } else {
            "▶ Running"
        };

        let time_info = if let Some(remaining) = state.time_remaining() {
            format!("{}s remaining", remaining.as_secs())
        } else {
            "N/A".to_owned()
        };

        let account_type = if config.is_premium {
            "Premium"
        } else {
            "Free"
        };

        let message = format!(
            "Status: {status}\n\
             Current: {current_desc}\n\
             Index: {}/{}\n\
             Time: {time_info}\n\
             Account: {account_type}",
            state.current_index + 1,
            config.len(),
        );

        CommandResult::success(message)
    }

    async fn handle_list(&self) -> CommandResult {
        let config = self.config.read().await;
        let state = self.scheduler_state.read().await;

        if config.is_empty() {
            return CommandResult::error("No descriptions configured.");
        }

        let mut lines = vec!["Configured descriptions:".to_owned()];

        for (i, desc) in config.descriptions.iter().enumerate() {
            let marker = if i == state.current_index {
                "→ "
            } else {
                "  "
            };
            let duration_str = format_duration(desc.duration_secs);
            lines.push(format!(
                "{marker}[{}] {} ({duration_str})",
                desc.id,
                truncate(&desc.text, 25)
            ));
        }

        CommandResult::success(lines.join("\n"))
    }

    async fn handle_view(&self, id: &str) -> CommandResult {
        let config = self.config.read().await;

        let desc = config
            .descriptions
            .iter()
            .find(|d| d.id == id)
            .or_else(|| {
                // Try as index
                id.parse::<usize>()
                    .ok()
                    .filter(|&i| i > 0 && i <= config.len())
                    .and_then(|i| config.get(i - 1))
            });

        match desc {
            Some(d) => {
                let char_count = d.char_count();
                let max_len = if config.is_premium {
                    MAX_BIO_LENGTH_PREMIUM
                } else {
                    MAX_BIO_LENGTH_FREE
                };

                let message = format!(
                    "Description [{}]:\n\
                     Text: \"{}\"\n\
                     Duration: {}\n\
                     Length: {}/{} chars",
                    d.id,
                    d.text,
                    format_duration(d.duration_secs),
                    char_count,
                    max_len
                );
                CommandResult::success(message)
            }
            None => CommandResult::error(format!(
                "Description not found: '{id}'. Use 'list' to see available descriptions."
            )),
        }
    }

    async fn handle_goto(&self, target: &str) -> CommandResult {
        let config = self.config.read().await;

        // Try to find by ID first
        let index = config
            .descriptions
            .iter()
            .position(|d| d.id == target)
            .or_else(|| {
                // Try to parse as index (1-based for user friendliness)
                target
                    .parse::<usize>()
                    .ok()
                    .filter(|&i| i > 0 && i <= config.len())
                    .map(|i| i - 1)
            });

        match index {
            Some(idx) => {
                drop(config); // Release read lock before acquiring write lock
                let mut state = self.scheduler_state.write().await;
                state.current_index = idx;
                state.skip_current = true; // Trigger immediate switch

                let config = self.config.read().await;
                let desc = &config.descriptions[idx];
                CommandResult::success_with_update(format!(
                    "✓ Jumping to [{}]: \"{}\"",
                    desc.id,
                    truncate(&desc.text, 30)
                ))
            }
            None => CommandResult::error(format!(
                "Description not found: '{target}'. Use 'list' to see available descriptions."
            )),
        }
    }

    async fn handle_pause(&self) -> CommandResult {
        let mut state = self.scheduler_state.write().await;

        if state.is_paused {
            return CommandResult::error("Already paused.");
        }

        state.is_paused = true;
        CommandResult::success("⏸ Description rotation paused.")
    }

    async fn handle_resume(&self) -> CommandResult {
        let mut state = self.scheduler_state.write().await;

        if !state.is_paused {
            return CommandResult::error("Already running.");
        }

        state.is_paused = false;
        CommandResult::success("▶ Description rotation resumed.")
    }

    async fn handle_reload(&self) -> CommandResult {
        match DescriptionConfig::load_from_file(&self.config_path) {
            Ok(new_config) => {
                if let Err(e) = new_config.validate() {
                    return CommandResult::error(format!("Validation failed: {e}"));
                }

                let mut config = self.config.write().await;
                let old_len = config.len();
                *config = new_config;
                let new_len = config.len();

                // Reset index if out of bounds
                let mut state = self.scheduler_state.write().await;
                if state.current_index >= new_len {
                    state.current_index = 0;
                }

                CommandResult::success(format!(
                    "✓ Reloaded configuration. {old_len} → {new_len} descriptions."
                ))
            }
            Err(e) => CommandResult::error(format!("Failed to reload: {e}")),
        }
    }

    fn handle_help(&self) -> CommandResult {
        let mut lines = vec![
            format!("Description Bot Commands (prefix: {})", self.prefix),
            String::new(),
        ];

        for (cmd, aliases, desc) in BotCommand::all_commands() {
            let alias_str = if aliases.is_empty() {
                String::new()
            } else {
                format!(" {aliases}")
            };
            lines.push(format!("  {cmd}{alias_str} - {desc}"));
        }

        CommandResult::success(lines.join("\n"))
    }

    async fn handle_set(&self, text: &str) -> CommandResult {
        // Validate text
        {
            let config = self.config.read().await;
            if let Err(e) = validate_description_text(text, &config) {
                return CommandResult::error(e);
            }
        }

        let mut state = self.scheduler_state.write().await;
        state.custom_description = Some(text.to_owned());
        state.skip_current = true;

        CommandResult::success_with_update(format!(
            "✓ Setting custom description: \"{}\"",
            truncate(text, 30)
        ))
    }

    async fn handle_add(&self, args: AddArgs) -> CommandResult {
        let mut config = self.config.write().await;

        // Check for duplicate ID
        if config.descriptions.iter().any(|d| d.id == args.id) {
            return CommandResult::error(format!(
                "Description with ID '{}' already exists. Use 'edit' to modify it.",
                args.id
            ));
        }

        // Validate text
        if let Err(e) = validate_description_text(&args.text, &config) {
            return CommandResult::error(e);
        }

        // Validate duration
        if args.duration_secs == 0 {
            return CommandResult::error("Duration must be greater than 0 seconds.");
        }

        // Validate ID (no spaces, not empty)
        if args.id.contains(char::is_whitespace) {
            return CommandResult::error("ID cannot contain spaces.");
        }

        // Create and add the new description
        let desc = Description::new(args.id.clone(), args.text.clone(), args.duration_secs);
        config.descriptions.push(desc);

        // Save to file
        if let Err(e) = config.save_to_file(&self.config_path) {
            warn!("Failed to save config: {}", e);
            return CommandResult::error(format!("Added but failed to save: {e}"));
        }

        CommandResult::success(format!(
            "✓ Added description [{}]: \"{}\" ({})",
            args.id,
            truncate(&args.text, 25),
            format_duration(args.duration_secs)
        ))
    }

    async fn handle_edit(&self, args: EditArgs) -> CommandResult {
        let mut config = self.config.write().await;

        // Find by index first (immutable operation)
        let index = config.descriptions.iter().position(|d| d.id == args.id);

        let Some(idx) = index else {
            return CommandResult::error(format!(
                "Description not found: '{}'. Use 'list' to see available descriptions.",
                args.id
            ));
        };

        // Validate new text
        if let Err(e) = validate_description_text(&args.text, &config) {
            return CommandResult::error(e);
        }

        // Now mutate
        let old_text = config.descriptions[idx].text.clone();
        config.descriptions[idx].text.clone_from(&args.text);

        // Save to file
        if let Err(e) = config.save_to_file(&self.config_path) {
            config.descriptions[idx].text = old_text; // Rollback
            warn!("Failed to save config: {}", e);
            return CommandResult::error(format!("Failed to save: {e}"));
        }

        CommandResult::success(format!(
            "✓ Updated [{}]: \"{}\"",
            args.id,
            truncate(&args.text, 30)
        ))
    }

    async fn handle_duration(&self, args: DurationArgs) -> CommandResult {
        let mut config = self.config.write().await;

        // Validate duration
        if args.duration_secs == 0 {
            return CommandResult::error("Duration must be greater than 0 seconds.");
        }

        // Find by index first
        let index = config.descriptions.iter().position(|d| d.id == args.id);

        let Some(idx) = index else {
            return CommandResult::error(format!(
                "Description not found: '{}'. Use 'list' to see available descriptions.",
                args.id
            ));
        };

        // Now mutate
        let old_duration = config.descriptions[idx].duration_secs;
        config.descriptions[idx].duration_secs = args.duration_secs;

        // Save to file
        if let Err(e) = config.save_to_file(&self.config_path) {
            config.descriptions[idx].duration_secs = old_duration; // Rollback
            warn!("Failed to save config: {}", e);
            return CommandResult::error(format!("Failed to save: {e}"));
        }

        CommandResult::success(format!(
            "✓ Updated [{}] duration: {} → {}",
            args.id,
            format_duration(old_duration),
            format_duration(args.duration_secs)
        ))
    }

    async fn handle_delete(&self, id: &str) -> CommandResult {
        let mut config = self.config.write().await;

        // Find the description index
        let index = config.descriptions.iter().position(|d| d.id == id);

        match index {
            Some(idx) => {
                let removed = config.descriptions.remove(idx);

                // Save to file
                if let Err(e) = config.save_to_file(&self.config_path) {
                    config.descriptions.insert(idx, removed); // Rollback
                    warn!("Failed to save config: {}", e);
                    return CommandResult::error(format!("Failed to save: {e}"));
                }

                // Adjust current index if needed
                drop(config);
                let mut state = self.scheduler_state.write().await;
                let config = self.config.read().await;

                if config.is_empty() {
                    state.current_index = 0;
                } else if state.current_index >= config.len() {
                    state.current_index = config.len() - 1;
                } else if state.current_index > idx {
                    state.current_index -= 1;
                }

                CommandResult::success(format!(
                    "✓ Deleted [{}]: \"{}\"",
                    id,
                    truncate(&removed.text, 30)
                ))
            }
            None => CommandResult::error(format!(
                "Description not found: '{id}'. Use 'list' to see available descriptions."
            )),
        }
    }

    #[allow(clippy::unused_self)]
    fn handle_info(&self) -> CommandResult {
        let version = env!("CARGO_PKG_VERSION");
        let message = format!(
            "Description User Bot v{version}\n\
             A Telegram userbot for dynamic profile descriptions.\n\
             Repository: https://github.com/user/description_user_bot"
        );
        CommandResult::success(message)
    }
}

/// Validates description text for use as a Telegram bio.
///
/// Checks:
/// - Not empty
/// - Not too long (based on premium status)
/// - Text only (no images, stickers, etc. - only printable characters)
/// - No control characters except newlines
fn validate_description_text(text: &str, config: &DescriptionConfig) -> Result<(), String> {
    // Check empty
    if text.is_empty() {
        return Err("Description text cannot be empty.".to_owned());
    }

    // Check length
    let max_len = if config.is_premium {
        MAX_BIO_LENGTH_PREMIUM
    } else {
        MAX_BIO_LENGTH_FREE
    };

    let char_count = text.chars().count();
    if char_count > max_len {
        return Err(format!(
            "Text too long: {char_count} chars (max: {max_len})"
        ));
    }

    // Check for invalid characters (control chars except common whitespace)
    for ch in text.chars() {
        if ch.is_control() && ch != '\n' && ch != '\t' {
            return Err(format!(
                "Invalid character detected (code: U+{:04X}). Only text is allowed.",
                ch as u32
            ));
        }
    }

    // Check for object replacement character (often used for embedded objects)
    if text.contains('\u{FFFC}') {
        return Err(
            "Embedded objects (images, files) are not allowed. Only text is supported.".to_owned(),
        );
    }

    // Check for zero-width characters that might hide content
    let suspicious_chars = [
        '\u{200B}', // Zero-width space
        '\u{200C}', // Zero-width non-joiner
        '\u{200D}', // Zero-width joiner
        '\u{2060}', // Word joiner
        '\u{FEFF}', // BOM / Zero-width no-break space
    ];

    for &ch in &suspicious_chars {
        if text.contains(ch) {
            return Err(format!(
                "Invisible/zero-width characters detected (U+{:04X}). Please use only visible text.",
                ch as u32
            ));
        }
    }

    Ok(())
}

/// Truncates a string to a maximum length, adding "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        s.to_owned()
    } else {
        format!("{}...", chars[..max_len].iter().collect::<String>())
    }
}

/// Formats a duration in seconds to a human-readable string.
fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        if mins == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {mins}m")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("Hello", 10), "Hello");
        assert_eq!(truncate("Hello, World!", 5), "Hello...");
        assert_eq!(truncate("Hi", 2), "Hi");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(60), "1m");
        assert_eq!(format_duration(90), "1m");
        assert_eq!(format_duration(3600), "1h");
        assert_eq!(format_duration(3660), "1h 1m");
        assert_eq!(format_duration(7200), "2h");
    }

    #[test]
    fn test_validate_description_text_valid() {
        let config = DescriptionConfig {
            descriptions: vec![],
            is_premium: false,
            auto_detect_premium: false,
        };
        assert!(validate_description_text("Hello World!", &config).is_ok());
        assert!(validate_description_text("Привет мир! 👋", &config).is_ok());
    }

    #[test]
    fn test_validate_description_text_empty() {
        let config = DescriptionConfig {
            descriptions: vec![],
            is_premium: false,
            auto_detect_premium: false,
        };
        assert!(validate_description_text("", &config).is_err());
    }

    #[test]
    fn test_validate_description_text_too_long() {
        let config = DescriptionConfig {
            descriptions: vec![],
            is_premium: false,
            auto_detect_premium: false,
        };
        let long_text = "a".repeat(71);
        assert!(validate_description_text(&long_text, &config).is_err());
    }

    #[test]
    fn test_validate_description_text_premium_allows_longer() {
        let config = DescriptionConfig {
            descriptions: vec![],
            is_premium: true,
            auto_detect_premium: false,
        };
        let text = "a".repeat(100);
        assert!(validate_description_text(&text, &config).is_ok());
    }

    #[test]
    fn test_validate_description_text_zero_width() {
        let config = DescriptionConfig {
            descriptions: vec![],
            is_premium: false,
            auto_detect_premium: false,
        };
        let text_with_zwsp = "Hello\u{200B}World";
        assert!(validate_description_text(text_with_zwsp, &config).is_err());
    }
}

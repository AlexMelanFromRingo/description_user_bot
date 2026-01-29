//! Command types and definitions.

use std::fmt;

/// Arguments for adding a new description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddArgs {
    pub id: String,
    pub duration_secs: u64,
    pub text: String,
}

/// Arguments for editing an existing description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditArgs {
    pub id: String,
    pub text: String,
}

/// Arguments for changing description duration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurationArgs {
    pub id: String,
    pub duration_secs: u64,
}

/// Available bot commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotCommand {
    /// Skip the current description and move to the next one.
    Skip,

    /// Show the current status (current description, time remaining, etc.).
    Status,

    /// List all configured descriptions.
    List,

    /// Show detailed view of a specific description.
    View(String),

    /// Jump to a specific description by ID or index.
    Goto(String),

    /// Pause the description rotation.
    Pause,

    /// Resume the description rotation.
    Resume,

    /// Reload the descriptions configuration file.
    Reload,

    /// Show help information.
    Help,

    /// Set a custom description temporarily.
    Set(String),

    /// Add a new description.
    Add(AddArgs),

    /// Edit an existing description's text.
    Edit(EditArgs),

    /// Change description duration.
    Duration(DurationArgs),

    /// Delete a description.
    Delete(String),

    /// Show information about the bot.
    Info,
}

impl BotCommand {
    /// Parses a command from a message text.
    ///
    /// Returns `None` if the message is not a valid command.
    #[must_use]
    pub fn parse(text: &str, prefix: &str) -> Option<Self> {
        let text = text.trim();

        // Check if message starts with the command prefix
        if !text.starts_with(prefix) {
            return None;
        }

        // Extract the command part after the prefix
        let after_prefix = text[prefix.len()..].trim_start();

        // Handle commands with arguments
        let (cmd, args) = match after_prefix.split_once(char::is_whitespace) {
            Some((cmd, args)) => (cmd.to_lowercase(), Some(args.trim())),
            None => (after_prefix.to_lowercase(), None),
        };

        match cmd.as_str() {
            "skip" | "next" => Some(Self::Skip),
            "status" | "stat" | "s" => Some(Self::Status),
            "list" | "ls" | "l" => Some(Self::List),
            "view" | "show" => args
                .filter(|a| !a.is_empty())
                .map(|a| Self::View(a.to_owned())),
            "goto" | "go" | "jump" => args
                .filter(|a| !a.is_empty())
                .map(|a| Self::Goto(a.to_owned())),
            "pause" | "stop" => Some(Self::Pause),
            "resume" | "start" | "continue" => Some(Self::Resume),
            "reload" | "refresh" => Some(Self::Reload),
            "help" | "h" | "?" => Some(Self::Help),
            "set" => args
                .filter(|a| !a.is_empty())
                .map(|a| Self::Set(a.to_owned())),
            "add" | "new" => Self::parse_add(args?),
            "edit" | "change" => Self::parse_edit(args?),
            "duration" | "time" => Self::parse_duration(args?),
            "delete" | "remove" | "rm" | "del" => args
                .filter(|a| !a.is_empty())
                .map(|a| Self::Delete(a.to_owned())),
            "info" | "about" | "version" => Some(Self::Info),
            _ => None,
        }
    }

    /// Parses add command arguments: `<id> <duration_secs> <text>`
    fn parse_add(args: &str) -> Option<Self> {
        let mut parts = args.splitn(3, char::is_whitespace);
        let id = parts.next()?.to_owned();
        let duration_str = parts.next()?;
        let text = parts.next()?.trim().to_owned();

        if id.is_empty() || text.is_empty() {
            return None;
        }

        let duration_secs = duration_str.parse().ok()?;

        Some(Self::Add(AddArgs {
            id,
            duration_secs,
            text,
        }))
    }

    /// Parses edit command arguments: `<id> <text>`
    fn parse_edit(args: &str) -> Option<Self> {
        let (id, text) = args.split_once(char::is_whitespace)?;
        let id = id.to_owned();
        let text = text.trim().to_owned();

        if id.is_empty() || text.is_empty() {
            return None;
        }

        Some(Self::Edit(EditArgs { id, text }))
    }

    /// Parses duration command arguments: `<id> <duration_secs>`
    fn parse_duration(args: &str) -> Option<Self> {
        let mut parts = args.split_whitespace();
        let id = parts.next()?.to_owned();
        let duration_str = parts.next()?;

        if id.is_empty() {
            return None;
        }

        let duration_secs = duration_str.parse().ok()?;

        Some(Self::Duration(DurationArgs { id, duration_secs }))
    }

    /// Returns the command name as it appears in help.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Skip => "skip",
            Self::Status => "status",
            Self::List => "list",
            Self::View(_) => "view",
            Self::Goto(_) => "goto",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Reload => "reload",
            Self::Help => "help",
            Self::Set(_) => "set",
            Self::Add(_) => "add",
            Self::Edit(_) => "edit",
            Self::Duration(_) => "duration",
            Self::Delete(_) => "delete",
            Self::Info => "info",
        }
    }

    /// Returns the command description for help.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::Skip => "Skip current description, move to next",
            Self::Status => "Show current status and time remaining",
            Self::List => "List all configured descriptions",
            Self::View(_) => "View details of a specific description",
            Self::Goto(_) => "Jump to a specific description (by ID or index)",
            Self::Pause => "Pause description rotation",
            Self::Resume => "Resume description rotation",
            Self::Reload => "Reload descriptions from file",
            Self::Help => "Show this help message",
            Self::Set(_) => "Set a custom description temporarily",
            Self::Add(_) => "Add a new description",
            Self::Edit(_) => "Edit an existing description",
            Self::Duration(_) => "Change description duration",
            Self::Delete(_) => "Delete a description",
            Self::Info => "Show bot information",
        }
    }

    /// Returns all available commands with their descriptions.
    #[must_use]
    pub fn all_commands() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            ("skip", "", "Skip current description, move to next"),
            ("status", "(s)", "Show current status and time remaining"),
            ("list", "(ls)", "List all configured descriptions"),
            ("view <id>", "", "View details of a specific description"),
            ("goto <id>", "", "Jump to a specific description"),
            ("pause", "", "Pause description rotation"),
            ("resume", "", "Resume description rotation"),
            ("reload", "", "Reload descriptions from file"),
            ("set <text>", "", "Set a custom description temporarily"),
            ("add <id> <sec> <text>", "", "Add a new description"),
            ("edit <id> <text>", "", "Edit description text"),
            ("duration <id> <sec>", "", "Change description duration"),
            ("delete <id>", "(rm)", "Delete a description"),
            ("info", "", "Show bot information"),
            ("help", "(h, ?)", "Show this help message"),
        ]
    }
}

impl fmt::Display for BotCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::View(id) => write!(f, "view {id}"),
            Self::Goto(target) => write!(f, "goto {target}"),
            Self::Set(text) => write!(f, "set {text}"),
            Self::Add(args) => write!(f, "add {} {} {}", args.id, args.duration_secs, args.text),
            Self::Edit(args) => write!(f, "edit {} {}", args.id, args.text),
            Self::Duration(args) => write!(f, "duration {} {}", args.id, args.duration_secs),
            Self::Delete(id) => write!(f, "delete {id}"),
            _ => write!(f, "{}", self.name()),
        }
    }
}

/// Result of command execution.
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Whether the command was successful.
    pub success: bool,

    /// Response message to show the user.
    pub message: String,

    /// Whether to trigger an immediate description update.
    pub trigger_update: bool,
}

impl CommandResult {
    /// Creates a successful result.
    #[must_use]
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            trigger_update: false,
        }
    }

    /// Creates a successful result that triggers an update.
    #[must_use]
    pub fn success_with_update(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            trigger_update: true,
        }
    }

    /// Creates an error result.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            trigger_update: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREFIX: &str = "/description_bot";

    #[test]
    fn test_parse_skip() {
        assert_eq!(
            BotCommand::parse("/description_bot skip", PREFIX),
            Some(BotCommand::Skip)
        );
        assert_eq!(
            BotCommand::parse("/description_bot next", PREFIX),
            Some(BotCommand::Skip)
        );
    }

    #[test]
    fn test_parse_status() {
        assert_eq!(
            BotCommand::parse("/description_bot status", PREFIX),
            Some(BotCommand::Status)
        );
        assert_eq!(
            BotCommand::parse("/description_bot s", PREFIX),
            Some(BotCommand::Status)
        );
    }

    #[test]
    fn test_parse_goto_with_arg() {
        assert_eq!(
            BotCommand::parse("/description_bot goto morning", PREFIX),
            Some(BotCommand::Goto("morning".to_owned()))
        );
    }

    #[test]
    fn test_parse_goto_without_arg() {
        assert_eq!(BotCommand::parse("/description_bot goto", PREFIX), None);
    }

    #[test]
    fn test_parse_set_with_arg() {
        assert_eq!(
            BotCommand::parse("/description_bot set Hello World", PREFIX),
            Some(BotCommand::Set("Hello World".to_owned()))
        );
    }

    #[test]
    fn test_parse_add() {
        assert_eq!(
            BotCommand::parse("/description_bot add test_id 3600 Hello World", PREFIX),
            Some(BotCommand::Add(AddArgs {
                id: "test_id".to_owned(),
                duration_secs: 3600,
                text: "Hello World".to_owned(),
            }))
        );
    }

    #[test]
    fn test_parse_edit() {
        assert_eq!(
            BotCommand::parse("/description_bot edit test_id New text here", PREFIX),
            Some(BotCommand::Edit(EditArgs {
                id: "test_id".to_owned(),
                text: "New text here".to_owned(),
            }))
        );
    }

    #[test]
    fn test_parse_delete() {
        assert_eq!(
            BotCommand::parse("/description_bot delete test_id", PREFIX),
            Some(BotCommand::Delete("test_id".to_owned()))
        );
        assert_eq!(
            BotCommand::parse("/description_bot rm test_id", PREFIX),
            Some(BotCommand::Delete("test_id".to_owned()))
        );
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(
            BotCommand::parse("/description_bot duration test_id 7200", PREFIX),
            Some(BotCommand::Duration(DurationArgs {
                id: "test_id".to_owned(),
                duration_secs: 7200,
            }))
        );
    }

    #[test]
    fn test_parse_wrong_prefix() {
        assert_eq!(BotCommand::parse("/other_bot skip", PREFIX), None);
        assert_eq!(BotCommand::parse("skip", PREFIX), None);
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(
            BotCommand::parse("/description_bot SKIP", PREFIX),
            Some(BotCommand::Skip)
        );
        assert_eq!(
            BotCommand::parse("/description_bot Status", PREFIX),
            Some(BotCommand::Status)
        );
    }

    #[test]
    fn test_parse_with_extra_whitespace() {
        assert_eq!(
            BotCommand::parse("  /description_bot   skip  ", PREFIX),
            Some(BotCommand::Skip)
        );
    }
}

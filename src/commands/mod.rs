//! Command handling module.
//!
//! Processes user commands sent to the bot via Telegram messages.
//! Commands use the `/description_bot` prefix.

mod handler;
mod types;

pub use handler::CommandHandler;
pub use types::{BotCommand, CommandResult};

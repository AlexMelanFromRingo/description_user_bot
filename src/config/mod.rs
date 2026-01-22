//! Configuration module for the description bot.
//!
//! Handles loading, validation, and management of bot configuration
//! including descriptions, timing, and Telegram API credentials.

mod descriptions;
mod settings;

pub use descriptions::{Description, DescriptionConfig, ValidationError};
pub use settings::{BotSettings, TelegramConfig};

/// Maximum bio length for regular Telegram users.
pub const MAX_BIO_LENGTH_FREE: usize = 70;

/// Maximum bio length for Telegram Premium users.
pub const MAX_BIO_LENGTH_PREMIUM: usize = 140;

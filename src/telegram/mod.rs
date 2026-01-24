//! Telegram client wrapper module.
//!
//! Provides high-level abstractions for interacting with Telegram,
//! including authentication, profile updates, and rate limiting.

mod client;
mod rate_limiter;

pub use client::{
    PwdToken as PasswordToken, QrAuthResult, RawUpdatesReceiver, TelegramBot, TelegramError,
    Token as LoginToken,
};
pub use grammers_client::update::Update;
pub use rate_limiter::RateLimiter;

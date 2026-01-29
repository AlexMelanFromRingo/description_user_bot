//! Description User Bot Library
//!
//! A Telegram userbot for dynamic profile description updates.
//!
//! This crate provides the core functionality for:
//! - Loading and validating description configurations
//! - Connecting to Telegram via `MTProto`
//! - Rotating profile descriptions on a schedule
//! - Handling user commands via chat messages

pub mod commands;
pub mod config;
pub mod scheduler;
pub mod telegram;

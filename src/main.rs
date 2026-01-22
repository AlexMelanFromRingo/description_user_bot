//! Description User Bot - Main Entry Point
//!
//! A Telegram userbot that dynamically updates your profile description
//! based on configured rotation schedules.

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use dialoguer::{Input, Password};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

use description_user_bot::commands::CommandHandler;
use description_user_bot::config::{BotSettings, DescriptionConfig, TelegramConfig};
use description_user_bot::scheduler::{DescriptionScheduler, SchedulerMessage, SchedulerState};
use description_user_bot::telegram::{TelegramBot, TelegramError};

/// Telegram userbot for dynamic profile description updates.
#[derive(Parser, Debug)]
#[command(name = "description_bot")]
#[command(about = "Dynamically update your Telegram profile description")]
#[command(version)]
struct Args {
    /// Path to the descriptions JSON configuration file.
    #[arg(short, long, default_value = "descriptions.json")]
    config: String,

    /// Path to the .env file for environment variables.
    #[arg(long, default_value = ".env")]
    env_file: String,

    /// Log level (trace, debug, info, warn, error).
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Generate an example configuration file and exit.
    #[arg(long)]
    generate_config: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    init_logging(&args.log_level);

    // Handle example config generation
    if args.generate_config {
        return generate_example_config();
    }

    // Load environment variables
    if let Err(e) = dotenvy::from_filename(&args.env_file) {
        debug!("Could not load .env file ({}): {}", args.env_file, e);
    }

    // Load configurations
    let tg_config = TelegramConfig::from_env()
        .context("Failed to load Telegram configuration from environment")?;

    let bot_settings = BotSettings::from_env_with_defaults();

    let mut desc_config = DescriptionConfig::load_from_file(&args.config)
        .context("Failed to load descriptions configuration")?;

    info!(
        "Loaded {} descriptions (auto_detect_premium: {})",
        desc_config.len(),
        desc_config.auto_detect_premium
    );

    // Connect to Telegram
    let bot = TelegramBot::connect(&tg_config, bot_settings.min_update_interval_secs)
        .await
        .context("Failed to connect to Telegram")?;

    // Handle authentication if needed
    if !bot.is_authorized().await.context("Failed to check authorization")? {
        authenticate(&bot, &tg_config).await?;
    }

    // Auto-detect premium status if enabled
    if desc_config.auto_detect_premium {
        match bot.is_premium().await {
            Ok(is_premium) => {
                desc_config.set_premium(is_premium);
                info!(
                    "Auto-detected premium status: {}",
                    if is_premium { "Premium" } else { "Free" }
                );
            }
            Err(e) => {
                tracing::warn!("Failed to auto-detect premium status: {}. Using config value.", e);
            }
        }
    }

    // Validate after premium status is determined
    desc_config
        .validate()
        .context("Description configuration validation failed")?;

    info!(
        "Configuration validated (premium: {}, max_length: {})",
        desc_config.is_premium,
        desc_config.max_bio_length()
    );

    let bot = Arc::new(bot);
    let config = Arc::new(RwLock::new(desc_config));
    let state = Arc::new(RwLock::new(SchedulerState::new()));

    // Create scheduler channel
    let (scheduler_tx, scheduler_rx) = mpsc::channel::<SchedulerMessage>(32);

    // Create command handler (not fully used yet - requires updates stream integration)
    let _command_handler = Arc::new(CommandHandler::new(
        bot_settings.command_prefix.clone(),
        Arc::clone(&state),
        Arc::clone(&config),
        args.config.clone(),
    ));

    // Create scheduler
    let scheduler = DescriptionScheduler::new(
        Arc::clone(&bot),
        Arc::clone(&config),
        Arc::clone(&state),
    );

    info!("Starting description bot...");
    info!("Command prefix: {}", bot_settings.command_prefix);

    // Spawn scheduler task
    let scheduler_handle = tokio::spawn(async move {
        scheduler.run(scheduler_rx).await;
    });

    // For now, since we can't easily get updates handle after connect,
    // we'll run without update handling and just let the scheduler work
    info!("Bot is running. Use Ctrl+C to stop.");
    info!("Note: Command handling requires updates stream which is not yet fully integrated.");

    // Wait for Ctrl+C
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
        }
    }

    // Cleanup
    info!("Shutting down...");
    let _ = scheduler_tx.send(SchedulerMessage::Shutdown).await;
    let _ = scheduler_handle.await;
    bot.disconnect();

    Ok(())
}

/// Initializes the logging subsystem.
fn init_logging(level: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

/// Generates an example configuration file.
fn generate_example_config() -> Result<()> {
    let example = DescriptionConfig::example();
    example.save_to_file("descriptions.example.json")?;

    println!("✓ Example configuration written to: descriptions.example.json");
    println!("\nTo use this bot:");
    println!("1. Copy descriptions.example.json to descriptions.json");
    println!("2. Edit the descriptions to your liking");
    println!("3. Create a .env file with TG_API_ID and TG_API_HASH");
    println!("4. Run: description_bot");

    Ok(())
}

/// Handles Telegram authentication.
async fn authenticate(bot: &TelegramBot, config: &TelegramConfig) -> Result<()> {
    info!("Authentication required");

    let phone: String = Input::new()
        .with_prompt("Enter your phone number (with country code)")
        .interact_text()?;

    let token = bot
        .request_login_code(&phone, &config.api_hash)
        .await
        .context("Failed to request login code")?;

    info!("Login code sent to your Telegram app");

    let code: String = Input::new()
        .with_prompt("Enter the login code")
        .interact_text()?;

    match bot.sign_in(&token, &code).await {
        Ok(()) => {
            info!("Successfully signed in!");
            Ok(())
        }
        Err(TelegramError::PasswordRequired(password_token)) => {
            info!("Two-factor authentication is enabled");

            let hint = password_token.hint().unwrap_or("no hint");
            info!("Password hint: {}", hint);

            let password: String = Password::new()
                .with_prompt("Enter your 2FA password")
                .interact()?;

            bot.check_password(password_token, &password)
                .await
                .context("2FA authentication failed")?;

            info!("Successfully signed in with 2FA!");
            Ok(())
        }
        Err(e) => Err(e).context("Authentication failed"),
    }
}

/// Truncates a string for logging.
#[allow(dead_code)]
fn truncate_log(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_owned()
    } else {
        format!("{}...", s.chars().take(max_len).collect::<String>())
    }
}

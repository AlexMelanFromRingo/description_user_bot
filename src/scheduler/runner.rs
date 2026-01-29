//! Description scheduler runner.
//!
//! The scheduler follows a simple state machine:
//! 1. Check if expired (deadline passed or no deadline)
//! 2. If expired and not paused:
//!    - If custom description is set → use it, then clear it
//!    - Else if has deadline (regular expiration) → advance to next
//!    - Else (no deadline, e.g. after goto/skip) → use current index
//! 3. Apply the description via API
//! 4. On success → set new deadline and save state
//!
//! Commands modify state and SAVE immediately:
//! - goto/skip: set index + clear deadline + save
//! - pause/resume: set flag + save
//! - set: set custom description + clear deadline + save

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{RwLock, mpsc};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use super::SchedulerState;
use crate::config::DescriptionConfig;
use crate::telegram::{TelegramBot, TelegramError};

/// Messages that can be sent to the scheduler.
#[derive(Debug, Clone)]
pub enum SchedulerMessage {
    /// Trigger an immediate update check.
    TriggerUpdate,
    /// Stop the scheduler.
    Shutdown,
}

/// Description rotation scheduler.
pub struct DescriptionScheduler {
    /// Telegram bot client.
    bot: Arc<TelegramBot>,

    /// Description configuration.
    config: Arc<RwLock<DescriptionConfig>>,

    /// Scheduler state.
    state: Arc<RwLock<SchedulerState>>,

    /// Path to save persistent state.
    state_path: String,

    /// Check interval for state changes.
    check_interval: Duration,
}

impl DescriptionScheduler {
    /// Creates a new description scheduler.
    #[must_use]
    pub fn new(
        bot: Arc<TelegramBot>,
        config: Arc<RwLock<DescriptionConfig>>,
        state: Arc<RwLock<SchedulerState>>,
        state_path: String,
    ) -> Self {
        Self {
            bot,
            config,
            state,
            state_path,
            check_interval: Duration::from_secs(1),
        }
    }

    /// Sets the check interval for state changes.
    #[must_use]
    pub const fn with_check_interval(mut self, interval: Duration) -> Self {
        self.check_interval = interval;
        self
    }

    /// Runs the scheduler loop.
    pub async fn run(&self, mut rx: mpsc::Receiver<SchedulerMessage>) {
        info!("Description scheduler started");

        let mut check_timer = interval(self.check_interval);

        loop {
            tokio::select! {
                _ = check_timer.tick() => {
                    self.tick().await;
                }
                msg = rx.recv() => {
                    match msg {
                        Some(SchedulerMessage::TriggerUpdate) => {
                            debug!("Received trigger update message");
                            self.tick().await;
                        }
                        Some(SchedulerMessage::Shutdown) | None => {
                            info!("Scheduler shutting down");
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Single tick of the scheduler.
    async fn tick(&self) {
        // Step 1: Quick check if we should even try
        {
            let state = self.state.read().await;
            if state.is_paused || !state.is_expired() {
                return;
            }
        }

        // Step 2: Determine what to update (READ ONLY - don't modify state yet)
        let (text, duration_secs, description_id, should_advance, has_custom) = {
            let state = self.state.read().await;
            let config = self.config.read().await;

            // Re-check under lock
            if state.is_paused || !state.is_expired() {
                return;
            }

            if config.is_empty() {
                warn!("No descriptions configured");
                return;
            }

            // Figure out what we'll update (without modifying state)
            if let Some(ref custom) = state.custom_description {
                // Custom description
                (custom.clone(), 3600u64, "custom".to_owned(), false, true)
            } else {
                // Regular rotation
                let should_advance = state.has_deadline();
                let next_index = if should_advance {
                    (state.current_index + 1) % config.len()
                } else {
                    state.current_index
                };

                let desc = config.get(next_index).or_else(|| config.get(0));
                let Some(desc) = desc else {
                    error!("No description available");
                    return;
                };

                (
                    desc.text.clone(),
                    desc.duration_secs,
                    desc.id.clone(),
                    should_advance,
                    false,
                )
            }
        };

        // Step 3: Make API call (no locks held)
        debug!(
            "Updating bio to [{}]: \"{}\"",
            description_id,
            truncate(&text, 30)
        );

        match self.bot.update_bio(&text).await {
            Ok(()) => {
                // Step 4: On SUCCESS, modify state and save
                let mut state = self.state.write().await;
                let config = self.config.read().await;

                // Apply the changes we decided on
                if has_custom {
                    state.custom_description = None;
                } else if should_advance {
                    state.advance(config.len());
                }

                state.set_deadline(duration_secs);

                // Save state to disk
                if let Err(e) = state.to_persistent().save(&self.state_path) {
                    warn!("Failed to save state: {}", e);
                }

                info!(
                    "Bio updated to [{}], next update in {} seconds",
                    description_id, duration_secs
                );
            }
            Err(TelegramError::RateLimited(seconds)) => {
                debug!("Rate limited, {} seconds remaining", seconds);
                // Don't modify state - scheduler will retry on next tick
            }
            Err(TelegramError::FloodWait(seconds)) => {
                warn!("Flood wait from Telegram: {} seconds", seconds);
                // Don't modify state - will retry later
            }
            Err(e) => {
                error!("Failed to update bio: {}", e);
                // Don't modify state - will retry on next tick
            }
        }
    }

    /// Gets a reference to the scheduler state.
    #[must_use]
    pub fn state(&self) -> &Arc<RwLock<SchedulerState>> {
        &self.state
    }

    /// Gets a reference to the configuration.
    #[must_use]
    pub fn config(&self) -> &Arc<RwLock<DescriptionConfig>> {
        &self.config
    }
}

/// Truncates a string for display.
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_owned()
    } else {
        format!("{}...", s.chars().take(max_len).collect::<String>())
    }
}

impl std::fmt::Debug for DescriptionScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DescriptionScheduler")
            .field("check_interval", &self.check_interval)
            .finish_non_exhaustive()
    }
}

//! Description scheduler runner.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, RwLock};
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
    ///
    /// This function runs until a shutdown message is received or the
    /// receiver is closed.
    pub async fn run(&self, mut rx: mpsc::Receiver<SchedulerMessage>) {
        info!("Description scheduler started");

        let mut check_timer = interval(self.check_interval);

        loop {
            tokio::select! {
                _ = check_timer.tick() => {
                    self.check_and_update().await;
                }
                msg = rx.recv() => {
                    match msg {
                        Some(SchedulerMessage::TriggerUpdate) => {
                            debug!("Received trigger update message");
                            self.force_update().await;
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

    /// Checks if an update is needed and performs it.
    async fn check_and_update(&self) {
        let state = self.state.read().await;

        // Don't update if paused
        if state.is_paused {
            return;
        }

        // Check if current description has expired
        if !state.is_expired() {
            return;
        }

        drop(state); // Release read lock
        self.perform_update().await;
    }

    /// Forces an immediate update.
    async fn force_update(&self) {
        self.perform_update().await;
    }

    /// Performs the description update.
    async fn perform_update(&self) {
        let config = self.config.read().await;

        if config.is_empty() {
            warn!("No descriptions configured, skipping update");
            return;
        }

        // Get the description text to use
        let mut state = self.state.write().await;

        let (text, duration) = if let Some(custom) = state.custom_description.take() {
            // Use custom description with a default duration
            (custom, Duration::from_secs(3600)) // 1 hour default for custom
        } else {
            // Advance to next ONLY if this is a regular expiration (has_timing = true)
            // If timing was cleared (by goto/skip), don't advance - they already set the index
            if state.is_expired() && state.has_timing() {
                state.advance(config.len());
            }

            let desc = if let Some(d) = config.get(state.current_index) { d } else {
                state.current_index = 0;
                if let Some(d) = config.get(0) { d } else {
                    error!("Failed to get description at index 0");
                    return;
                }
            };

            (desc.text.clone(), Duration::from_secs(desc.duration_secs))
        };

        let current_index = state.current_index;
        drop(state); // Release write lock before API call
        drop(config);

        // Perform the API call
        info!("Updating bio (index: {})", current_index);

        match self.bot.update_bio(&text).await {
            Ok(()) => {
                let mut state = self.state.write().await;
                state.mark_started(duration);

                // Save persistent state
                if let Err(e) = state.to_persistent().save(&self.state_path) {
                    warn!("Failed to save state: {}", e);
                }

                info!("Bio updated successfully, next update in {:?}", duration);
            }
            Err(TelegramError::FloodWait(seconds)) => {
                warn!("Flood wait: {} seconds, will retry later", seconds);
                // The rate limiter in TelegramBot handles the wait
            }
            Err(e) => {
                error!("Failed to update bio: {}", e);
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

impl std::fmt::Debug for DescriptionScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DescriptionScheduler")
            .field("check_interval", &self.check_interval)
            .finish_non_exhaustive()
    }
}

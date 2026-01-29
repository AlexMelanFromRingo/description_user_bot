//! Scheduler state management.
//!
//! This module uses a simple approach:
//! - Store "deadline" (Unix timestamp when current description expires)
//! - On each tick, check if current time >= deadline
//! - No Instant gymnastics, no race conditions with timing

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Gets current Unix timestamp in seconds.
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Persistent state that survives restarts.
/// This is stored as JSON in state.json.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistentState {
    /// Current description index.
    pub current_index: usize,
    /// Whether rotation is paused.
    pub is_paused: bool,
    /// Unix timestamp when current description expires (deadline).
    /// None means "needs immediate update".
    pub expires_at_unix: Option<u64>,
    /// Pending custom description (survives restarts).
    pub custom_description: Option<String>,
}

impl PersistentState {
    /// Loads state from a JSON file, returns default if not found.
    pub fn load(path: impl AsRef<Path>) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Saves state to a JSON file.
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }
}

/// Runtime state of the description scheduler.
/// Simple and straightforward - deadline based timing.
#[derive(Debug, Clone, Default)]
pub struct SchedulerState {
    /// Current description index in the list.
    pub current_index: usize,

    /// Whether rotation is paused.
    pub is_paused: bool,

    /// Custom description to use instead of the configured one.
    /// Set by "set" command, consumed on next update.
    pub custom_description: Option<String>,

    /// Unix timestamp when current description expires.
    /// None = needs immediate update (first run or after goto/skip).
    expires_at_unix: Option<u64>,

    /// Duration of current description (for status display).
    current_duration_secs: Option<u64>,
}

impl SchedulerState {
    /// Creates a new scheduler state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates state from persistent state loaded from disk.
    #[must_use]
    pub fn from_persistent(persistent: &PersistentState) -> Self {
        Self {
            current_index: persistent.current_index,
            is_paused: persistent.is_paused,
            custom_description: persistent.custom_description.clone(),
            expires_at_unix: persistent.expires_at_unix,
            current_duration_secs: None, // Recalculated on first update
        }
    }

    /// Converts to persistent state for saving.
    #[must_use]
    pub fn to_persistent(&self) -> PersistentState {
        PersistentState {
            current_index: self.current_index,
            is_paused: self.is_paused,
            expires_at_unix: self.expires_at_unix,
            custom_description: self.custom_description.clone(),
        }
    }

    /// Checks if the current description has expired (deadline passed).
    #[must_use]
    pub fn is_expired(&self) -> bool {
        match self.expires_at_unix {
            Some(deadline) => now_unix() >= deadline,
            None => true, // No deadline = needs update
        }
    }

    /// Checks if we have a valid deadline set.
    #[must_use]
    pub fn has_deadline(&self) -> bool {
        self.expires_at_unix.is_some()
    }

    /// Returns the time remaining until expiration.
    #[must_use]
    pub fn time_remaining(&self) -> Option<Duration> {
        let deadline = self.expires_at_unix?;
        let now = now_unix();
        if now >= deadline {
            Some(Duration::ZERO)
        } else {
            Some(Duration::from_secs(deadline - now))
        }
    }

    /// Returns the total duration of current description.
    #[must_use]
    pub fn current_duration(&self) -> Option<Duration> {
        self.current_duration_secs.map(Duration::from_secs)
    }

    /// Advances to the next description index (wrapping around).
    pub fn advance(&mut self, total_count: usize) {
        if total_count == 0 {
            return;
        }
        self.current_index = (self.current_index + 1) % total_count;
    }

    /// Sets the deadline for current description.
    /// Call this AFTER successful bio update.
    pub fn set_deadline(&mut self, duration_secs: u64) {
        let now = now_unix();
        self.expires_at_unix = Some(now + duration_secs);
        self.current_duration_secs = Some(duration_secs);
    }

    /// Clears the deadline (triggers immediate update on next tick).
    /// Used by goto/skip commands.
    pub fn clear_deadline(&mut self) {
        self.expires_at_unix = None;
        self.current_duration_secs = None;
    }

    /// Sets the index directly (for goto command).
    pub fn set_index(&mut self, index: usize) {
        self.current_index = index;
        self.clear_deadline();
    }

    /// Clears the custom description.
    pub fn clear_custom(&mut self) {
        self.custom_description = None;
    }

    /// Resets the scheduler state to initial values.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let state = SchedulerState::default();
        assert_eq!(state.current_index, 0);
        assert!(!state.is_paused);
        assert!(state.custom_description.is_none());
        assert!(!state.has_deadline());
        assert!(state.is_expired()); // No deadline = expired
    }

    #[test]
    fn test_advance_wraps_around() {
        let mut state = SchedulerState::new();
        state.current_index = 2;
        state.advance(3);
        assert_eq!(state.current_index, 0);
    }

    #[test]
    fn test_advance_increments() {
        let mut state = SchedulerState::new();
        state.advance(5);
        assert_eq!(state.current_index, 1);
    }

    #[test]
    fn test_is_expired_no_deadline() {
        let state = SchedulerState::new();
        assert!(state.is_expired());
    }

    #[test]
    fn test_deadline_in_future() {
        let mut state = SchedulerState::new();
        state.set_deadline(3600); // 1 hour from now

        assert!(!state.is_expired());
        assert!(state.has_deadline());

        let remaining = state.time_remaining();
        assert!(remaining.is_some());
        // Should be close to 3600 seconds (allow 5 sec margin)
        let secs = remaining.unwrap().as_secs();
        assert!(secs >= 3595 && secs <= 3600);
    }

    #[test]
    fn test_set_index_clears_deadline() {
        let mut state = SchedulerState::new();
        state.set_deadline(3600);
        assert!(state.has_deadline());

        state.set_index(5);
        assert_eq!(state.current_index, 5);
        assert!(!state.has_deadline()); // Deadline cleared
    }

    #[test]
    fn test_persistent_roundtrip() {
        let mut state = SchedulerState::new();
        state.current_index = 3;
        state.is_paused = true;
        state.custom_description = Some("test".to_owned());
        state.set_deadline(1000);

        let persistent = state.to_persistent();
        let restored = SchedulerState::from_persistent(&persistent);

        assert_eq!(restored.current_index, 3);
        assert!(restored.is_paused);
        assert_eq!(restored.custom_description, Some("test".to_owned()));
        assert!(restored.has_deadline());
    }
}

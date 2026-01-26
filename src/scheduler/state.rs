//! Scheduler state management.

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Persistent state that survives restarts.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistentState {
    /// Current description index.
    pub current_index: usize,
    /// Whether rotation is paused.
    pub is_paused: bool,
    /// Unix timestamp when current description started (seconds).
    pub started_at_unix: Option<u64>,
    /// Duration of current description in seconds.
    pub duration_secs: Option<u64>,
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

/// Gets current Unix timestamp in seconds.
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// State of the description scheduler.
#[derive(Debug)]
#[derive(Default)]
pub struct SchedulerState {
    /// Current description index.
    pub current_index: usize,

    /// Whether rotation is paused.
    pub is_paused: bool,

    /// Custom description to use instead of the configured one.
    pub custom_description: Option<String>,

    /// When the current description was set (Instant for precise timing).
    current_started_at: Option<Instant>,

    /// Unix timestamp when started (for persistence).
    started_at_unix: Option<u64>,

    /// Duration of the current description.
    current_duration: Option<Duration>,
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
        let mut state = Self {
            current_index: persistent.current_index,
            is_paused: persistent.is_paused,
            started_at_unix: persistent.started_at_unix,
            current_duration: persistent.duration_secs.map(Duration::from_secs),
            ..Self::default()
        };

        // Restore Instant from Unix timestamp if available
        if let (Some(started_unix), Some(_duration)) = (persistent.started_at_unix, persistent.duration_secs) {
            let now = now_unix();
            if started_unix <= now {
                let elapsed_secs = now - started_unix;
                // Create an Instant that represents "elapsed_secs ago"
                state.current_started_at = Instant::now().checked_sub(Duration::from_secs(elapsed_secs));
            }
        }

        state
    }

    /// Converts to persistent state for saving.
    #[must_use]
    pub fn to_persistent(&self) -> PersistentState {
        PersistentState {
            current_index: self.current_index,
            is_paused: self.is_paused,
            started_at_unix: self.started_at_unix,
            duration_secs: self.current_duration.map(|d| d.as_secs()),
        }
    }

    /// Returns the time remaining for the current description.
    #[must_use]
    pub fn time_remaining(&self) -> Option<Duration> {
        let started = self.current_started_at?;
        let duration = self.current_duration?;
        let elapsed = started.elapsed();

        if elapsed >= duration {
            Some(Duration::ZERO)
        } else {
            Some(duration - elapsed)
        }
    }

    /// Checks if the current description has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        match (self.current_started_at, self.current_duration) {
            (Some(started), Some(duration)) => started.elapsed() >= duration,
            _ => true, // No timing info means it's ready for the first run
        }
    }

    /// Checks if timing info is available (not the first run).
    #[must_use]
    pub fn has_timing(&self) -> bool {
        self.current_duration.is_some()
    }

    /// Advances to the next description index (wrapping around).
    pub fn advance(&mut self, total_count: usize) {
        if total_count == 0 {
            return;
        }
        self.current_index = (self.current_index + 1) % total_count;
        self.custom_description = None;
    }

    /// Marks the start of a new description with the given duration.
    pub fn mark_started(&mut self, duration: Duration) {
        self.current_started_at = Some(Instant::now());
        self.started_at_unix = Some(now_unix());
        self.current_duration = Some(duration);
    }

    /// Clears the custom description.
    pub fn clear_custom(&mut self) {
        self.custom_description = None;
    }

    /// Clears timing info (for goto/skip operations).
    pub fn clear_timing(&mut self) {
        self.current_started_at = None;
        self.started_at_unix = None;
        self.current_duration = None;
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
        assert!(!state.has_timing());
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
    fn test_is_expired_no_timing() {
        let state = SchedulerState::new();
        assert!(state.is_expired());
    }

    #[test]
    fn test_time_remaining() {
        let mut state = SchedulerState::new();
        state.mark_started(Duration::from_secs(60));

        let remaining = state.time_remaining();
        assert!(remaining.is_some());
        assert!(remaining.unwrap() <= Duration::from_secs(60));
    }
}

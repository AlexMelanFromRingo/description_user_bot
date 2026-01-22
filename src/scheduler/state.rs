//! Scheduler state management.

use std::time::{Duration, Instant};

/// State of the description scheduler.
#[derive(Debug)]
#[derive(Default)]
pub struct SchedulerState {
    /// Current description index.
    pub current_index: usize,

    /// Whether rotation is paused.
    pub is_paused: bool,

    /// Whether to skip the current description immediately.
    pub skip_current: bool,

    /// Custom description to use instead of the configured one.
    pub custom_description: Option<String>,

    /// When the current description was set.
    pub current_started_at: Option<Instant>,

    /// Duration of the current description.
    pub current_duration: Option<Duration>,
}


impl SchedulerState {
    /// Creates a new scheduler state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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

    /// Advances to the next description index (wrapping around).
    pub fn advance(&mut self, total_count: usize) {
        if total_count == 0 {
            return;
        }
        self.current_index = (self.current_index + 1) % total_count;
        self.skip_current = false;
        self.custom_description = None;
    }

    /// Marks the start of a new description with the given duration.
    pub fn mark_started(&mut self, duration: Duration) {
        self.current_started_at = Some(Instant::now());
        self.current_duration = Some(duration);
        self.skip_current = false;
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
        assert!(!state.skip_current);
        assert!(state.custom_description.is_none());
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

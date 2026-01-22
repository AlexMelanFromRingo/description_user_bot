//! Description rotation scheduler module.
//!
//! Manages the automatic rotation of profile descriptions
//! according to configured durations.

mod state;
mod runner;

pub use state::SchedulerState;
pub use runner::{DescriptionScheduler, SchedulerMessage};

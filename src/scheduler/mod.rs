//! Description rotation scheduler module.
//!
//! Manages the automatic rotation of profile descriptions
//! according to configured durations.

mod runner;
mod state;

pub use runner::{DescriptionScheduler, SchedulerMessage};
pub use state::{PersistentState, SchedulerState};

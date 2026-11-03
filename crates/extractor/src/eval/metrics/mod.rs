//! Pure scoring functions over timestamps and text. No I/O; every
//! function here is unit-tested on hand-built inputs.

pub mod boundaries;
pub mod commands;
pub mod steps;

pub use boundaries::{match_boundaries, BoundaryCounts};
pub use commands::{backtick_commands, exact_match, normalize, spoken_form, spoken_in_window};
pub use steps::{recover_steps, Span, StepCounts};

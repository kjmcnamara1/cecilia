//! Score data model for muse.
//!
//! The model is pure data with no I/O. Time is measured in ticks, [`TICKS_PER_QUARTER`] per
//! quarter note. Every note on a fretted track resolves to a MIDI pitch through its track's
//! tuning, so tablature and standard notation are two views of the same notes.

pub mod chant;
mod duration;
pub mod performance;
mod score;

pub use duration::{Duration, DurationValue, Tuplet};
pub use performance::{Performance, play_order};
pub use score::*;

/// Ticks per quarter note used throughout the model.
pub const TICKS_PER_QUARTER: u32 = 960;

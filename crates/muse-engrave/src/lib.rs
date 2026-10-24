//! Notation layout for muse: a score becomes pages of backend-neutral drawing items
//! ([`display::Page`]), which [`output`] renders to SVG, PNG and PDF and the GUI draws with vello.

pub mod chant;
pub mod display;
pub mod fonts;
pub mod layout;
pub mod output;

pub use layout::{BeatPos, Layout, Mode, Options, engrave};

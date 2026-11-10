//! Headless application layer for cecilia: everything a frontend needs except drawing and audio
//! output. Frontends feed [`Key`]s to [`App::handle_key`], draw from the [`App`] state, and carry
//! out the [`Effect`]s it requests.

mod app;
pub mod beatsync;
mod chant;
mod chant_text;
pub mod chantsync;
pub mod clipboard;
pub mod command;
pub mod complete;
pub mod config;
pub mod dashboard;
mod design;
pub mod dialog;
pub mod edit;
#[cfg(test)]
mod edit_tests;
mod editor;
mod external;
pub mod history;
pub mod icons;
pub mod instrument;
pub mod keys;
pub mod layout;
pub mod marks;
pub mod mouse;
pub mod options;
pub mod palette;
#[cfg(test)]
mod palette_tools_tests;
pub mod picker;
pub mod playalong;
pub mod playback;
pub mod progress;
pub mod properties;
pub mod statusline;
mod swap;
mod textobj;
pub mod theme;
pub mod toolbar;
pub mod tracks;
mod wavesync;
pub mod workspace;

pub use app::{
    App, Buffer, BufferTab, Cursor, Effect, LinkedNote, Message, Palette, PaletteItem, Pane,
    PaneKind, ScrollTo, View, ViewInfo, beat_at_tick, beats, cursor_tick, file_kind, staff_index,
    truncate_middle,
};
pub use chant_text::TextMarks;
pub use command::{Command, Strip};
pub use config::{Config, Mode};
pub use editor::Clip;
pub use keys::{Key, KeyCode};
pub use swap::default_swap_dir;

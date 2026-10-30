//! Headless application layer for muse: everything a frontend needs except drawing and audio
//! output. Frontends feed [`Key`]s to [`App::handle_key`], draw from the [`App`] state, and carry
//! out the [`Effect`]s it requests.

mod app;
mod chant;
pub mod command;
pub mod complete;
pub mod config;
pub mod dialog;
pub mod edit;
mod editor;
pub mod history;
pub mod icons;
pub mod instrument;
pub mod keys;
pub mod layout;
pub mod marks;
pub mod picker;
pub mod playalong;
pub mod playback;
pub mod progress;
pub mod properties;
pub mod statusline;
pub mod theme;
pub mod toolbar;
pub mod tracks;
pub mod workspace;

pub use app::{
    App, Buffer, BufferTab, Cursor, Effect, Message, Palette, PaletteItem, Pane, PaneKind, View,
    beat_at_tick, beats, cursor_tick, file_kind, staff_index, truncate_middle,
};
pub use command::{Command, Strip};
pub use config::{Config, Mode};
pub use editor::Clip;
pub use keys::{Key, KeyCode};

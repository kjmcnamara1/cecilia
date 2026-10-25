//! Headless application layer for muse: everything a frontend needs except drawing and audio
//! output. Frontends feed [`Key`]s to [`App::handle_key`], draw from the [`App`] state, and carry
//! out the [`Effect`]s it requests.

mod app;
mod chant;
pub mod command;
pub mod complete;
pub mod config;
pub mod edit;
mod editor;
pub mod history;
pub mod keys;
pub mod layout;
pub mod playalong;
pub mod playback;
pub mod workspace;

pub use app::{
    App, Buffer, BufferTab, Cursor, Effect, Message, Palette, PaletteItem, Pane, View,
    beat_at_tick, beats, cursor_tick, file_kind, truncate_middle,
};
pub use command::Command;
pub use config::{Config, Mode};
pub use editor::Clip;
pub use keys::{Key, KeyCode};

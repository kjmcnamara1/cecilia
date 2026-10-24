//! Headless application layer for muse: everything a frontend needs except drawing and audio
//! output. Frontends feed [`Key`]s to [`App::handle_key`], draw from the [`App`] state, and carry
//! out the [`Effect`]s it requests.

mod app;
pub mod command;
pub mod config;
pub mod keys;
pub mod layout;

pub use app::{App, Buffer, Cursor, Effect, Message, Pane, Tab, beat_at_tick, beats, cursor_tick};
pub use command::Command;
pub use config::{Config, Mode};
pub use keys::{Key, KeyCode};

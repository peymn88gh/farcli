//! farcli runs a Target app on a hidden terminal and repaints the Terminal
//! from a Hidden screen of what the app drew.

pub mod app_profile;
pub mod app_runner;
mod emulator;
pub mod fake_terminal;
pub mod hidden_screen;
pub mod painter;
pub mod recording;
pub mod run;
pub mod screen;
pub mod session;
pub mod terminal;

/// The size of a Terminal, or of the hidden terminal, in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

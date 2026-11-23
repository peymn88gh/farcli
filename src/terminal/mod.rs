//! The real Terminal farcli was started in: its size, its cursor, what the
//! user does in it, and a way to send it bytes.

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{Restore, Terminal};

use crate::Size;

/// Something the user did in the Terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEvent {
    /// Keys the user pressed, as the bytes the Terminal sent for them.
    Keys(Vec<u8>),
    /// The user gave the window a new size.
    Resized(Size),
}

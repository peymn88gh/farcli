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
    /// What the Terminal sent, as the bytes it sent: keys the user pressed,
    /// text the user pasted, mouse reports and answers to what the Target
    /// app asked.
    Sent(Vec<u8>),
    /// The user gave the window a new size.
    Resized(Size),
}

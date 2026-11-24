//! The hotkey: the one key that switches the correction off and on in a
//! running session. It is F12 pressed alone.
//!
//! F12 was chosen because nothing between the keyboard and the Target app
//! takes it. Checked on 2026-11-24 against each one's own list of defaults:
//!
//! - Claude Code (its keybindings page): no function key is bound at all.
//! - Codex 0.171.0 and 0.172.0 (`codex-rs/tui/src/keymap.rs`): F2, F3, F4,
//!   F7, F8 and F9 are bound; F12 is not.
//! - Windows Terminal (`defaults.json`): only F11 and Alt+F4.
//! - WezTerm (its default key table): no function key.
//! - VS Code's terminal: it keeps a key from the program only when the key
//!   runs one of the commands in its `commandsToSkipShell` list, and none
//!   of VS Code's F12 commands is on that list.
//!
//! Every key with Ctrl or Alt was ruled out: the two apps and the three
//! Terminals between them use most of those.

use crate::terminal_input::{Input, Key};

/// Windows' number for the F12 key.
const F12: u16 = 123;

/// The modifier bits that say Shift, Ctrl or Alt is held, as Windows
/// numbers them. The others say that a lock key is on, or which of two
/// like keys was pressed.
const HELD_WITH: u32 = 0x01 | 0x02 | 0x04 | 0x08 | 0x10;

/// F12 as a Terminal sends it that has no Windows form for keys.
const F12_IN_PLAIN_FORM: &[u8] = b"\x1b[24~";

/// Watches what the Terminal sends for the hotkey.
#[derive(Default)]
pub(crate) struct Hotkey {
    /// The key went down and has not come up yet.
    held: bool,
    /// What comes now is pasted text, in which nothing is a key.
    in_paste: bool,
}

/// What something the Terminal sent is to the hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pressed {
    /// The hotkey was pressed just now.
    Now,
    /// Part of a press already counted: the key repeating while it is held
    /// down, or coming up. It is the hotkey's and not the Target app's.
    Still,
    /// Not the hotkey.
    No,
}

impl Hotkey {
    pub fn new() -> Hotkey {
        Hotkey::default()
    }

    pub fn sees(&mut self, input: &Input) -> Pressed {
        match input {
            Input::PasteStarts => {
                self.in_paste = true;
                Pressed::No
            }
            Input::PasteEnds => {
                self.in_paste = false;
                Pressed::No
            }
            _ if self.in_paste => Pressed::No,
            Input::Key(key) if key.key == F12 => self.sees_f12(key),
            Input::Other(code) if code == F12_IN_PLAIN_FORM => Pressed::Now,
            _ => Pressed::No,
        }
    }

    fn sees_f12(&mut self, key: &Key) -> Pressed {
        if !key.down {
            // The end of a press that was the hotkey's is the hotkey's too,
            // whatever was pressed in the meantime.
            return match std::mem::take(&mut self.held) {
                true => Pressed::Still,
                false => Pressed::No,
            };
        }
        if key.modifiers & HELD_WITH != 0 {
            return Pressed::No;
        }
        match std::mem::replace(&mut self.held, true) {
            true => Pressed::Still,
            false => Pressed::Now,
        }
    }
}

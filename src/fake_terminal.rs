//! A stand-in for a Terminal, for tests and developer tools: it takes the
//! bytes farcli sends a Terminal and tells what a person would then see.

use crate::emulator::Emulator;
use crate::screen::{row_text, Cell, Position, Row};
use crate::Size;

/// How many scrolled-off rows the fake Terminal keeps for scrolling back.
const SCROLLBACK_ROWS: usize = 10_000;

pub struct FakeTerminal {
    emulator: Emulator,
}

impl FakeTerminal {
    pub fn new(size: Size) -> FakeTerminal {
        FakeTerminal {
            emulator: Emulator::new(size, SCROLLBACK_ROWS),
        }
    }

    /// Takes bytes written to the Terminal.
    pub fn receive(&mut self, bytes: &[u8]) {
        self.emulator.feed(bytes);
    }

    /// The user gives the Terminal's window a new size.
    pub fn resize(&mut self, size: Size) {
        self.emulator.resize(size);
    }

    pub fn size(&self) -> Size {
        self.emulator.size()
    }

    pub fn cursor(&self) -> Position {
        self.emulator.cursor()
    }

    pub fn cursor_visible(&self) -> bool {
        self.emulator.cursor_visible()
    }

    pub fn on_alternate_screen(&self) -> bool {
        self.emulator.on_alternate_screen()
    }

    /// The rows on screen, top to bottom.
    pub fn screen(&self) -> Vec<Row> {
        self.emulator.rows()
    }

    /// The text of each row on screen, without the empty cells at its end.
    pub fn screen_text(&self) -> Vec<String> {
        self.screen().iter().map(|row| row_text(row)).collect()
    }

    pub fn cell(&self, at: Position) -> Cell {
        self.screen()[usize::from(at.row)][usize::from(at.col)].clone()
    }

    /// The text of each row a person finds by scrolling back, oldest first.
    pub fn scrollback_text(&self) -> Vec<String> {
        self.emulator
            .scrollback()
            .iter()
            .map(|row| row_text(row))
            .collect()
    }
}

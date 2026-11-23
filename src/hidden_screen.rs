//! Hidden screen: holds what the Target app has drawn: characters, styles
//! and the app's cursor.

mod app_output;

use crate::emulator::Emulator;
use crate::screen::{Position, Row, ScreenChange};
use crate::Size;

use app_output::{Piece, Splitter};

/// More rows than a Target app can scroll off the top with one piece of
/// output, so that none is lost before it is collected.
const UNCOLLECTED_ROWS: usize = 100_000;

/// The most output handed to the emulator before scrolled-off rows are
/// collected.
const PIECE_LEN: usize = 4096;

pub struct HiddenScreen {
    emulator: Emulator,
    splitter: Splitter,
    changes: Vec<ScreenChange>,
    mid_frame: bool,
}

/// Something a Target app wrote that is not drawing, and so is not held by
/// the Hidden screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aside {
    /// A code for the Terminal itself, to pass on unchanged: the window
    /// title, a keyboard or mouse mode, a question to the Terminal, the bell.
    ForTerminal(Vec<u8>),
    /// The answer to a question that only the Hidden screen can answer, such
    /// as where the app's cursor is.
    ForApp(Vec<u8>),
    /// The question of what kind of terminal this is, as it was written.
    /// Whoever knows who asked decides who answers.
    TerminalKindAsked(Vec<u8>),
}

impl HiddenScreen {
    /// A screen the app has drawn nothing on yet, with the app's cursor at
    /// `cursor`. Every cell starts out left alone (`Cell::left_alone`).
    pub fn new(size: Size, cursor: Position) -> HiddenScreen {
        let mut emulator = Emulator::new(size, UNCOLLECTED_ROWS);
        emulator.mark_all_left_alone();
        emulator.feed(format!("\x1b[{};{}H", cursor.row + 1, cursor.col + 1).as_bytes());
        HiddenScreen {
            emulator,
            splitter: Splitter::new(),
            changes: Vec::new(),
            mid_frame: false,
        }
    }

    /// Takes in what the Target app wrote, and returns the parts of it that
    /// are not drawing, in the order the app wrote them.
    pub fn app_wrote(&mut self, bytes: &[u8]) -> Vec<Aside> {
        let mut asides = Vec::new();
        for piece in self.splitter.split(bytes) {
            match piece {
                Piece::Drawing(bytes) => self.draw(&bytes),
                Piece::Erase(part, code) => {
                    self.changes.push(ScreenChange::Erased {
                        part,
                        cursor: self.cursor(),
                    });
                    self.emulator.feed(&code);
                    // The emulator may keep erased rows for scrolling back.
                    // Whether the Terminal does is the Terminal's business.
                    self.emulator.forget_scrollback();
                }
                Piece::ScreenSwitch(code) => {
                    let was_alternate = self.emulator.on_alternate_screen();
                    self.emulator.feed(&code);
                    let alternate = self.emulator.on_alternate_screen();
                    if alternate != was_alternate {
                        self.changes.push(ScreenChange::AlternateScreen(alternate));
                    }
                }
                Piece::ForTerminal(code) => asides.push(Aside::ForTerminal(code)),
                Piece::CursorPositionAsked => {
                    let cursor = self.cursor();
                    let answer = format!("\x1b[{};{}R", cursor.row + 1, cursor.col + 1);
                    asides.push(Aside::ForApp(answer.into_bytes()));
                }
                Piece::TerminalKindAsked(code) => asides.push(Aside::TerminalKindAsked(code)),
                Piece::FrameStarts => self.mid_frame = true,
                Piece::FrameEnds => self.mid_frame = false,
            }
        }
        asides
    }

    fn draw(&mut self, bytes: &[u8]) {
        for piece in bytes.chunks(PIECE_LEN) {
            self.emulator.feed(piece);
            let scrolled_off = self.emulator.scrollback();
            self.emulator.forget_scrollback();
            self.changes
                .extend(scrolled_off.into_iter().map(ScreenChange::ScrolledOff));
        }
    }

    /// What happened to the whole screen since this was last asked, in the
    /// order it happened. The rows are as they are after all of it.
    pub fn take_changes(&mut self) -> Vec<ScreenChange> {
        std::mem::take(&mut self.changes)
    }

    /// True while the app is part-way through drawing a frame that it asked
    /// not to be shown half done.
    pub fn mid_frame(&self) -> bool {
        self.mid_frame
    }

    /// The app gave up on telling when its frame is complete.
    pub fn end_frame(&mut self) {
        self.mid_frame = false;
    }

    pub fn size(&self) -> Size {
        self.emulator.size()
    }

    /// The hidden terminal, and so the app, was given a new size.
    pub fn resize(&mut self, size: Size) {
        // A Terminal rearranges its text for a new size, so nothing on it
        // is where it was: no cell can be left alone any more.
        self.emulator.forget_left_alone();
        self.emulator.resize(size);
        // Rows the new size pushed off the top are not the app scrolling.
        self.emulator.forget_scrollback();
    }

    /// The rows the app has drawn, top to bottom.
    pub fn rows(&self) -> Vec<Row> {
        self.emulator.rows()
    }

    /// Where the app's cursor is.
    pub fn cursor(&self) -> Position {
        self.emulator.cursor()
    }

    /// False while the app has hidden its cursor.
    pub fn cursor_visible(&self) -> bool {
        self.emulator.cursor_visible()
    }
}

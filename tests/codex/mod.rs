//! For tests of Codex's screen: a session with the Codex App profile, and
//! the bytes Codex writes for the parts of its screen.
//!
//! Codex writes text in Logical order, letters unjoined. It places each row
//! by its position and writes only the cells that changed since its last
//! frame, so a row often arrives in pieces.

#![allow(dead_code)]

use farcli::app_profile::AppProfile;
use farcli::fake_terminal::FakeTerminal;
use farcli::layout::{self, Direction};
use farcli::screen::{row_text, Cell};
use farcli::session::Session;
use farcli::Size;

/// A session running Codex, joined to the Terminal that shows its output.
pub struct Run {
    pub session: Session,
    pub terminal: FakeTerminal,
    /// A second Terminal that is sent exactly what the app wrote, with no
    /// farcli in between: it shows what the app drew.
    pub without_farcli: FakeTerminal,
}

impl Run {
    /// Starts Codex in an empty Terminal.
    pub fn start(cols: u16, rows: u16) -> Run {
        let terminal = FakeTerminal::new(Size { cols, rows });
        let session = Session::start(AppProfile::codex(), terminal.size(), terminal.cursor());
        Run {
            session,
            terminal,
            without_farcli: FakeTerminal::new(Size { cols, rows }),
        }
    }

    pub fn app_writes(&mut self, bytes: impl AsRef<[u8]>) {
        let reaction = self.session.app_wrote(bytes.as_ref());
        self.terminal.receive(&reaction.for_terminal);
        self.without_farcli.receive(bytes.as_ref());
    }

    /// The text of the row at `row`.
    pub fn row(&self, row: usize) -> String {
        self.terminal.screen_text().swap_remove(row)
    }

    /// The text of the row at `row` as the app drew it.
    pub fn drawn_row(&self, row: usize) -> String {
        self.without_farcli.screen_text().swap_remove(row)
    }

    /// True when the Terminal shows the row at `row` exactly as the app
    /// drew it: every character, colour and style.
    pub fn row_is_as_drawn(&self, row: usize) -> bool {
        self.terminal.screen()[row] == self.without_farcli.screen()[row]
    }

    /// True when the whole screen, and the cursor, are as the app drew them.
    pub fn screen_is_as_drawn(&self) -> bool {
        self.terminal.screen() == self.without_farcli.screen()
            && self.terminal.cursor() == self.without_farcli.cursor()
    }
}

/// Text that reads from the right, as it sits on a screen from the left.
pub fn from_the_right(text: &str) -> String {
    text.chars().rev().collect()
}

/// A row `cols` wide with `text` against its right edge.
pub fn at_right_edge(cols: usize, text: &str) -> String {
    format!("{text:>cols$}")
}

/// The bytes Codex writes to draw `text` on the row `row`, counted from 0,
/// from the first column, and to empty the rest of the row.
pub fn on_row(row: usize, text: &str) -> String {
    format!("\x1b[{};1H\x1b[0m{text}\x1b[0m\x1b[K", row + 1)
}

/// The first row of a reply: the reply mark, dim, and the text after it.
pub fn reply_start(text: &str) -> String {
    format!("\x1b[2m• \x1b[22m{text}")
}

/// Any other row of a reply's text: two empty columns and the text.
pub fn reply_row(text: &str) -> String {
    format!("  {text}")
}

/// A row of the message the user sent, or of the Input box, on the
/// background Codex gives it: `mark` in the first column and the text from
/// the third, the background filling the row.
pub fn on_background(mark: &str, text: &str, background: (u8, u8, u8)) -> String {
    let (red, green, blue) = background;
    format!("\x1b[48;2;{red};{green};{blue}m{mark} {text}\x1b[K")
}

/// The background of a message the user sent, and of the Input box.
pub const SENT: (u8, u8, u8) = (50, 50, 50);
pub const INPUT_BOX: (u8, u8, u8) = (41, 41, 41);

/// The bytes Codex writes for a whole screen: every row from the top, and
/// the rows left over emptied.
pub fn screen(rows: &[String]) -> String {
    let mut bytes = String::from("\x1b[?2026h");
    for (row, text) in rows.iter().enumerate() {
        bytes.push_str(&on_row(row, text));
    }
    bytes.push_str(&format!("\x1b[{};1H\x1b[0m\x1b[J", rows.len() + 1));
    bytes.push_str("\x1b[?2026l");
    bytes
}

fn takes_no_cell(character: char) -> bool {
    matches!(character, '\u{200c}' | '\u{200d}')
        || unicode_bidi::bidi_class(character) == unicode_bidi::BidiClass::NSM
}

/// How many cells a text takes.
pub fn cells(text: &str) -> usize {
    text.chars().filter(|c| !takes_no_cell(*c)).count()
}

/// The rows Codex wraps a paragraph onto in an area `width` cells wide: as
/// many words on each row as fit, and no space left at the end of a row.
pub fn wrapped(paragraph: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    let mut row = String::new();
    for word in paragraph.split(' ') {
        if row.is_empty() {
            row.push_str(word);
        } else if cells(&row) + 1 + cells(word) <= width {
            row.push(' ');
            row.push_str(word);
        } else {
            rows.push(std::mem::replace(&mut row, word.to_string()));
        }
    }
    rows.push(row);
    rows
}

/// The cells of a text in Logical order, as a Hidden screen holds them: a
/// Half-space or vowel mark rides on the letter before it.
pub fn logical_cells(text: &str) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::new();
    for character in text.chars() {
        match cells.last_mut() {
            Some(cell) if takes_no_cell(character) => cell.marks.push(character),
            _ => cells.push(Cell {
                character,
                ..Cell::blank()
            }),
        }
    }
    cells
}

/// The Paragraph direction of a text in Logical order.
pub fn direction_of(paragraph: &str) -> Direction {
    layout::paragraph_direction(&logical_cells(paragraph))
}

/// What Layout makes of one line of a paragraph given in Logical order, in
/// an area `width` cells wide: the text of the area, with the empty cells
/// at its right end left off.
pub fn laid_out(line: &str, direction: Direction, width: usize) -> String {
    let cells = logical_cells(line.trim_end_matches(' '));
    row_text(&layout::lay_out_line(&cells, direction, width).cells)
}

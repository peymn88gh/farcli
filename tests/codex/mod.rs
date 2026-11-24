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

/// The bytes Codex writes to empty the row `row`, counted from 0, and draw
/// `text` on it from the first column.
pub fn on_row(row: usize, text: &str) -> String {
    format!("\x1b[{};1H\x1b[0m\x1b[2K{text}\x1b[0m", row + 1)
}

/// The first row of a reply: the reply mark, dim, and the text after it.
pub fn reply_start(text: &str) -> String {
    format!("\x1b[2m• \x1b[22m{text}")
}

/// Any other row of a reply's text: two empty columns and the text.
pub fn reply_row(text: &str) -> String {
    format!("  {text}")
}

/// A row on a background that fills it, as Codex draws the message the
/// user sent and the Input box.
fn on_background(text: &str, background: (u8, u8, u8)) -> String {
    let (red, green, blue) = background;
    format!("\x1b[48;2;{red};{green};{blue}m{text}\x1b[K")
}

/// The first row of the message the user sent: the prompt mark, bold and
/// dim, and the text from the third column.
pub fn sent_start(text: &str) -> String {
    on_background(&format!("\x1b[1;2m› \x1b[22m{text}"), (50, 50, 50))
}

/// Any other row of the message the user sent.
pub fn sent_row(text: &str) -> String {
    on_background(&format!("  {text}"), (50, 50, 50))
}

/// The row of the Input box that holds its mark, bold, and its text.
pub fn input_box(text: &str) -> String {
    on_background(&format!("\x1b[1m›\x1b[22m {text}"), (41, 41, 41))
}

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

/// Text in one of the colours Codex's theme gives the words of a code
/// block, a list marker or a heading.
pub fn coloured(text: &str, colour: (u8, u8, u8)) -> String {
    let (red, green, blue) = colour;
    format!("\x1b[38;2;{red};{green};{blue}m{text}\x1b[39m")
}

pub fn bold(text: &str) -> String {
    format!("\x1b[1m{text}\x1b[22m")
}

pub fn dim(text: &str) -> String {
    format!("\x1b[2m{text}\x1b[22m")
}

pub fn italic(text: &str) -> String {
    format!("\x1b[3m{text}\x1b[23m")
}

/// Colours of the theme Codex chose in the recorded sessions.
pub const COMMENT: (u8, u8, u8) = (147, 153, 178);
pub const WORD: (u8, u8, u8) = (205, 214, 244);
pub const STRING: (u8, u8, u8) = (166, 227, 161);
pub const MARKER: (u8, u8, u8) = (148, 226, 213);
pub const HEADING: (u8, u8, u8) = (249, 226, 175);

/// The row under the Input box that names the model, which Codex draws in
/// colour. A session that shows it is one whose colours are switched on.
pub fn status_row() -> String {
    format!("  {}", coloured("GPT low", (246, 226, 183)))
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

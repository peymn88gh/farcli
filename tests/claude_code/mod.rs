//! For tests of Claude Code's screen: a session with the Claude Code App
//! profile, and the bytes Claude Code writes for a given reply.

#![allow(dead_code)]

pub mod input_box;

use farcli::app_profile::AppProfile;
use farcli::fake_terminal::FakeTerminal;
use farcli::layout::{self, Direction};
use farcli::screen::{row_text, Cell, Position};
use farcli::session::Session;
use farcli::Size;

/// A session running Claude Code, joined to the Terminal that shows its
/// output.
pub struct Run {
    pub session: Session,
    pub terminal: FakeTerminal,
    /// A second Terminal that is sent exactly what the app wrote, with no
    /// farcli in between: it shows what the app drew.
    pub without_farcli: FakeTerminal,
}

impl Run {
    /// Starts Claude Code in an empty Terminal.
    pub fn start(cols: u16, rows: u16) -> Run {
        let terminal = FakeTerminal::new(Size { cols, rows });
        let session = Session::start(
            AppProfile::claude_code(),
            terminal.size(),
            terminal.cursor(),
        );
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

pub fn at(row: u16, col: u16) -> Position {
    Position { row, col }
}

/// Text that reads from the right, as it sits on a screen from the left.
pub fn from_the_right(text: &str) -> String {
    text.chars().rev().collect()
}

/// A row `cols` wide with `text` against its right edge.
pub fn at_right_edge(cols: usize, text: &str) -> String {
    format!("{text:>cols$}")
}

/// What Claude Code on Windows draws for a row whose text is `logical`, as
/// the recordings of ticket #3 found it: Screen order by the Unicode
/// reordering rules with the row's own first letter deciding its direction,
/// letters unjoined, brackets not turned round, and a Half-space or vowel
/// mark kept after its letter.
///
/// A library works this out. farcli's own reordering has no part in it.
pub fn as_drawn(logical: &str) -> String {
    // A letter and what rides on it are one thing to the app: it orders
    // the row as if nothing rode on any letter, and each letter takes its
    // riders along. (Seen in a live session: a Half-space typed last on a
    // row drawn left to right stays on the Farsi letter before it.)
    let mut letters: Vec<String> = Vec::new();
    for character in logical.chars() {
        match letters.last_mut() {
            Some(letter) if takes_no_cell(character) => letter.push(character),
            _ => letters.push(character.to_string()),
        }
    }
    let alone: String = letters
        .iter()
        .filter_map(|letter| letter.chars().next())
        .collect();
    if alone.is_empty() {
        return String::new();
    }
    let info = unicode_bidi::BidiInfo::new(&alone, None);
    let paragraph = &info.paragraphs[0];
    let (levels, runs) = info.visual_runs(paragraph, paragraph.range.clone());
    // Where each letter starts in the text the library was given.
    let starts: Vec<usize> = alone.char_indices().map(|(at, _)| at).collect();
    let mut drawn = String::new();
    for run in runs {
        let first = starts.partition_point(|start| *start < run.start);
        let end = starts.partition_point(|start| *start < run.end);
        if levels[run.start].is_rtl() {
            drawn.extend(letters[first..end].iter().rev().map(String::as_str));
        } else {
            drawn.extend(letters[first..end].iter().map(String::as_str));
        }
    }
    drawn
}

fn takes_no_cell(character: char) -> bool {
    matches!(character, '\u{200c}' | '\u{200d}')
        || unicode_bidi::bidi_class(character) == unicode_bidi::BidiClass::NSM
}

/// The bytes Claude Code writes for one row of text starting at the column
/// `col`: each word placed by column, the spaces between words never drawn.
pub fn placed(col: usize, drawn: &str) -> String {
    let mut bytes = String::new();
    let mut at = col;
    let mut in_word = false;
    for character in drawn.chars() {
        if character == ' ' {
            in_word = false;
            at += 1;
            continue;
        }
        if !in_word {
            bytes.push_str(&format!("\x1b[{}G", at + 1));
            in_word = true;
        }
        bytes.push(character);
        if !takes_no_cell(character) {
            at += 1;
        }
    }
    bytes
}

/// How many cells a text takes.
pub fn cells(text: &str) -> usize {
    text.chars().filter(|c| !takes_no_cell(*c)).count()
}

/// The rows Claude Code wraps a paragraph onto in an area `width` cells
/// wide, in Logical order: as many words on each row as fit, and the space
/// the wrap broke at left on the end of the row when there is room for it.
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
            if cells(&row) < width {
                row.push(' ');
            }
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

/// The bytes Claude Code writes for a reply. Each block is a paragraph, a
/// list, a table or a code block, given row by row as Claude Code wrapped
/// it, in Logical order; a row that the wrap ended keeps the space the wrap
/// left at its end. The reply mark goes before the first row and an empty
/// row after each block.
pub fn reply(blocks: &[&[&str]]) -> String {
    let mut bytes = String::new();
    let mut marked = false;
    for block in blocks {
        for row in *block {
            if !marked {
                bytes.push('●');
                marked = true;
            }
            bytes.push_str(&placed(2, &as_drawn(row)));
            bytes.push_str("\r\n");
        }
        bytes.push_str("\r\n");
    }
    bytes
}

/// The bytes Claude Code writes for a reply made of paragraphs, in a
/// Terminal `cols` wide: it wraps each in the reply's area, which runs from
/// the third column to the last.
pub fn reply_of(paragraphs: &[&str], cols: usize) -> String {
    let wrapped: Vec<Vec<String>> = paragraphs
        .iter()
        .map(|paragraph| wrapped(paragraph, cols - 2))
        .collect();
    let rows: Vec<Vec<&str>> = wrapped
        .iter()
        .map(|rows| rows.iter().map(String::as_str).collect())
        .collect();
    let blocks: Vec<&[&str]> = rows.iter().map(Vec::as_slice).collect();
    reply(&blocks)
}

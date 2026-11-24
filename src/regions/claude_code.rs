//! The Region finder for Claude Code CLI, as version 2.1.322 draws its
//! screen. It reads rows from the top down:
//!
//! - A reply starts on a row with the reply mark in the first column. Its
//!   text, and every row the text wraps onto, starts in the third column
//!   and may run to the last one. Paragraphs, lists, tables and code blocks
//!   inside a reply are set apart by an empty row.
//! - A message the user sent starts with the prompt mark in the first
//!   column and carries on in the third.
//! - A row of line-drawing dashes from the first column opens the Input
//!   box, and the next such row closes it.
//! - Any other row with something in its first two columns is the app's
//!   interface, and ends the message above it.
//!
//! Inside a reply, code is told from prose by how the app colours it. The
//! app draws a code block with no border and no indentation of its own, so
//! a block is code when:
//!
//! - a word of it has one of the Terminal's numbered colours, which the app
//!   gives to the keywords, numbers, strings, comments and built-in names
//!   of a language it knows (and otherwise only to links);
//! - or a row of it is drawn wholly in a colour, as every row of a code
//!   block with no language named is;
//! - or a row of it is drawn wholly dim, as the name of a language the app
//!   does not know is, above the code;
//! - or a row of it starts further in than text does.
//!
//! Rows with none of these are prose. Where the app wrapped a text they are
//! one paragraph, and where the writer broke the line (a poem, an address)
//! each line is a paragraph of its own. Two things stay unsure and are left
//! as drawn: several lines under one another from an app run with its
//! colours switched off, where code has no colour either, and whatever
//! follows a part of a reply that scrolled off the top and was not prose.
//!
//! One kind of code looks exactly like lines of prose and is read as such:
//! a block in a language the app knows, or named `text`, in which no word
//! is a keyword, number, string, comment or built-in name.
//!
//! The same display serves the classic mode, where rows scroll off the top
//! for good, and the fullscreen mode, where the app draws the conversation
//! again at whatever place the user has scrolled to.

use std::ops::Range;

use crate::layout::{self, Bearing};
use crate::screen::{Cell, Color, Row};

use super::{Kind, ParagraphStart, Region};

/// The first column of a message's text.
const TEXT_COL: usize = 2;

/// Marks that start a reply: the one drawn on Windows and Linux, and the
/// one drawn on macOS.
const REPLY_MARKS: [char; 2] = ['●', '⏺'];

/// Marks that start a sent message and the Input box: the usual one, and
/// the one drawn where the Terminal is not known to have the first.
const PROMPT_MARKS: [char; 2] = ['❯', '>'];

/// The dash the Input box's borders are drawn with.
const RULE: char = '─';

/// The mark at the start of what a tool gave back.
const TOOL_RESULT_MARK: char = '⎿';

/// The bar the app draws before each line of a quotation. Like a list
/// marker it stays at the left, out of the app's reordering.
const QUOTATION_BAR: char = '▎';

pub(crate) struct Finder {
    /// What lies above the first row the finder is given next.
    above: Above,
}

/// What lies above a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Above {
    /// No message: the app's interface, or nothing of the app's at all.
    Outside,
    /// Not known. It may be the middle of a message.
    Unknown,
    /// A reply, which the row may carry on.
    Reply(Option<Cut>),
    /// A message the user sent.
    Sent,
    /// The Input box's upper border.
    InputBox,
}

/// The part of a reply that the row above belongs to, when the row below
/// may belong to it too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cut {
    Paragraph,
    /// Anything that is not a paragraph: its other rows are not one either.
    Other,
}

/// What a row is, going by its first columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Empty,
    Rule,
    ReplyHead,
    PromptHead,
    /// Nothing in the first two columns, and text further on.
    Indented,
    Other,
}

impl Finder {
    /// A finder for a screen the app has drawn nothing on yet.
    pub fn new() -> Finder {
        Finder {
            above: Above::Outside,
        }
    }

    /// A finder for rows of which nothing is known about what lies above
    /// them: the fullscreen display, scrolled to any place.
    pub fn knowing_nothing_above() -> Finder {
        Finder {
            above: Above::Unknown,
        }
    }

    /// The app cleared the screen: nothing of a message is above its top.
    pub fn screen_cleared(&mut self) {
        self.above = Above::Outside;
    }

    /// The regions of `rows`, which follow one another from the top down.
    /// The first `leaving` rows are scrolling off the top of the screen:
    /// the finder keeps what it needs to carry on from the row after them.
    /// `draws_in_colour` says whether the app has drawn anything in colour
    /// in this session: with colours switched off, less can be told.
    pub fn find(&mut self, rows: &[Row], leaving: usize, draws_in_colour: bool) -> Vec<Region> {
        let mut walk = Walk {
            rows,
            draws_in_colour,
            regions: Vec::new(),
            state: match self.above {
                Above::Reply(_) => Above::Reply(None),
                above => above,
            },
            cut_above: match self.above {
                Above::Reply(cut) => cut,
                Above::Unknown => Some(Cut::Paragraph),
                _ => None,
            },
            started_unknown: self.above == Above::Unknown,
            block: Vec::new(),
            span_start: 0,
        };
        let mut state_at_split = self.above;
        for at in 0..rows.len() {
            if at == leaving {
                state_at_split = walk.state;
            }
            walk.step(at);
        }
        walk.close(rows.len());

        if leaving > 0 && leaving < rows.len() {
            // A region cut by the top of the screen carries on below it.
            let cut = walk
                .regions
                .iter()
                .find(|region| region.rows.start < leaving && region.rows.end > leaving);
            self.above = match cut.map(|region| region.kind) {
                Some(Kind::Paragraph(_)) => Above::Reply(Some(Cut::Paragraph)),
                Some(Kind::Code | Kind::List | Kind::Table | Kind::Unsure) => {
                    Above::Reply(Some(Cut::Other))
                }
                Some(Kind::SentMessage | Kind::InputBox) | None => state_at_split,
            };
        }
        walk.regions
    }
}

/// Where the message being typed is drawn inside an Input box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TypedMessage {
    /// The rows it is drawn on, between the box's two borders.
    pub rows: Range<usize>,
    /// The columns it is drawn in: from after the prompt mark to the last.
    pub cols: Range<usize>,
    /// How many cells of a row the app fills before it wraps the message.
    /// It keeps two columns free at the right edge.
    pub wrap_width: usize,
    /// Nothing has been typed: what the box shows, if anything, is the
    /// app's own hint of what could be typed, drawn dim.
    pub is_empty: bool,
}

/// The message in the Input box `region` of `rows`, when the box is one the
/// user types a message in: closed by a border above and below, the prompt
/// mark at the start of its first row and nothing under the mark on the
/// rows the message wraps onto. Anything else between two borders, such as
/// a question the app asks or the box in the mode that runs a command, is
/// not a message.
pub(crate) fn typed_message(rows: &[Row], region: &Region) -> Option<TypedMessage> {
    if region.kind != Kind::InputBox || region.rows.len() < 3 {
        return None;
    }
    let inside = region.rows.start + 1..region.rows.end - 1;
    if shape(&rows[region.rows.start]) != Shape::Rule || shape(&rows[inside.end]) != Shape::Rule {
        return None;
    }
    let first = &rows[inside.start];
    let marked = PROMPT_MARKS.contains(&first[0].character)
        && first
            .get(1)
            .is_some_and(|cell| is_empty(cell) || cell.character == '\u{a0}');
    let nothing_under_the_mark = rows[inside.start + 1..inside.end]
        .iter()
        .all(|row| row[..TEXT_COL.min(row.len())].iter().all(is_empty));
    if !marked || !nothing_under_the_mark || first.len() < TEXT_COL + 4 {
        return None;
    }
    let cols = TEXT_COL..first.len();
    let mut drawn = rows[inside.clone()]
        .iter()
        .flat_map(|row| &row[cols.clone()])
        .filter(|cell| !is_empty(cell))
        .peekable();
    let is_empty = drawn.peek().is_none() || drawn.all(|cell| cell.style.dim);
    Some(TypedMessage {
        rows: inside,
        wrap_width: cols.len() - 2,
        cols,
        is_empty,
    })
}

struct Walk<'a> {
    rows: &'a [Row],
    draws_in_colour: bool,
    regions: Vec<Region>,
    state: Above,
    /// The part of a reply that the first row may carry on from above the
    /// rows given, until a row shows that it does not.
    cut_above: Option<Cut>,
    started_unknown: bool,
    /// The rows of the part of a reply being gathered.
    block: Vec<usize>,
    /// Where the sent message or Input box being gathered starts.
    span_start: usize,
}

impl Walk<'_> {
    fn step(&mut self, at: usize) {
        let row = &self.rows[at];
        let shape = shape(row);
        if self.state == Above::InputBox {
            if shape == Shape::Rule {
                self.close(at + 1);
                self.state = Above::Outside;
            }
            return;
        }
        match shape {
            Shape::Rule => {
                self.close(at);
                self.state = Above::InputBox;
                self.span_start = at;
            }
            Shape::ReplyHead => {
                self.close(at);
                self.state = Above::Reply(None);
                if !covered(row) {
                    self.block.push(at);
                }
            }
            Shape::PromptHead => {
                self.close(at);
                self.state = Above::Sent;
                self.span_start = at;
            }
            // The fullscreen display lays a header row over the top of the
            // conversation. What follows it may still be the middle of a
            // message.
            Shape::Other if self.started_unknown && at == 0 => {}
            Shape::Other => {
                self.close(at);
                self.state = Above::Outside;
            }
            Shape::Empty => self.close_block(),
            Shape::Indented => match self.state {
                // Something laid over a row, such as the fullscreen
                // display's button for jumping to the bottom, makes the
                // row no part of what it covers.
                Above::Reply(_) | Above::Unknown if covered(row) => self.close_block(),
                Above::Reply(_) | Above::Unknown => self.block.push(at),
                _ => {}
            },
        }
    }

    /// Ends whatever is being gathered, at the row `end`.
    fn close(&mut self, end: usize) {
        self.close_block();
        // A sent message's text starts where a reply's does, after the
        // mark. The Input box is the whole width, borders and all.
        let (kind, first_col) = match self.state {
            Above::Sent => (Kind::SentMessage, TEXT_COL),
            Above::InputBox => (Kind::InputBox, 0),
            _ => return,
        };
        if self.span_start < end {
            let cols = self.rows[self.span_start].len();
            self.regions.push(Region {
                rows: self.span_start..end,
                cols: first_col.min(cols)..cols,
                kind,
            });
        }
    }

    /// Ends the part of a reply being gathered and names what it holds.
    fn close_block(&mut self) {
        // Whatever was cut by the top of the rows given ends with the
        // first block, or before it.
        let top = self.cut_above.take();
        let Some(&first) = self.block.first() else {
            return;
        };
        let last = self.block[self.block.len() - 1];
        let rows = &self.rows[first..=last];
        let cols = TEXT_COL..rows[0].len();
        let mut push = |from: usize, to: usize, kind: Kind| {
            self.regions.push(Region {
                rows: first + from..first + to,
                cols: cols.clone(),
                kind,
            });
        };
        self.block.clear();

        if top == Some(Cut::Other) {
            push(0, rows.len(), Kind::Unsure);
            return;
        }
        // From its first list item, table border, tool result or line of a
        // quotation on, a block is that and nothing else. Nothing lays out
        // a quotation yet, so it is left as drawn like a part that could
        // not be named.
        let kinds: Vec<RowKind> = rows.iter().map(|row| row_kind(row)).collect();
        let text_rows = kinds
            .iter()
            .position(|kind| !matches!(kind, RowKind::Text | RowKind::Deep))
            .unwrap_or(rows.len());
        if text_rows < rows.len() {
            let kind = match kinds[text_rows] {
                RowKind::ListItem => Kind::List,
                RowKind::Table => Kind::Table,
                _ => Kind::Unsure,
            };
            push(text_rows, rows.len(), kind);
        }
        let text = &rows[..text_rows];
        if text.is_empty() {
            return;
        }
        // Code keeps its own indentation, and the app colours it: a word
        // here and there in the Terminal's numbered colours, or every word
        // of a row. A paragraph starts every row at the left edge of its
        // area, and its plain words are in the Terminal's own colours.
        if kinds[..text_rows].contains(&RowKind::Deep)
            || text
                .iter()
                .any(|row| holds_code_colour(row) || is_wholly_coloured_or_dim(row))
        {
            push(0, text_rows, Kind::Code);
            return;
        }
        // The app wraps a paragraph only where the next word does not fit.
        // A row that left room for the first word of the row below it ends
        // a line: the writer broke the line there, as in a poem or an
        // address, and the next row starts a paragraph of its own.
        let width = cols.len();
        let mut lines = vec![0];
        lines.extend((1..text_rows).filter(|row| !wraps_into(&text[row - 1], &text[*row], width)));
        // Lines of code with no colours look the same as lines of prose.
        // The app's colours are what tells them apart, so with colours
        // switched off several lines under one another may be either, and
        // code must stay as drawn.
        if lines.len() > 1 && !self.draws_in_colour {
            push(0, text_rows, Kind::Unsure);
            return;
        }
        lines.push(text_rows);
        for (line, rows) in lines.windows(2).enumerate() {
            let begins = match (line, top) {
                (_, None) => ParagraphStart::Here,
                (0, Some(_)) => ParagraphStart::Above,
                (_, Some(_)) if self.started_unknown => ParagraphStart::HereUnderLinesOutOfSight,
                (_, Some(_)) => ParagraphStart::Here,
            };
            push(rows[0], rows[1], Kind::Paragraph(begins));
        }
    }
}

fn is_empty(cell: &Cell) -> bool {
    cell.is_left_alone() || cell.character == ' '
}

/// True when something with a background of its own is drawn on the row.
fn covered(row: &[Cell]) -> bool {
    row.iter().any(|cell| {
        !cell.is_left_alone() && (cell.style.background != Color::Default || cell.style.inverse)
    })
}

fn shape(row: &[Cell]) -> Shape {
    let Some(first) = row.iter().position(|cell| !is_empty(cell)) else {
        return Shape::Empty;
    };
    if first >= TEXT_COL {
        return Shape::Indented;
    }
    let mark = row[0].character;
    let then_empty = row.get(1).is_some_and(is_empty);
    if mark == RULE && row.iter().filter(|cell| cell.character == RULE).count() * 2 >= row.len() {
        Shape::Rule
    } else if first == 0 && REPLY_MARKS.contains(&mark) && then_empty {
        Shape::ReplyHead
    } else if first == 0
        && PROMPT_MARKS.contains(&mark)
        && row
            .get(1)
            .is_some_and(|cell| is_empty(cell) || cell.character == '\u{a0}')
    {
        Shape::PromptHead
    } else {
        Shape::Other
    }
}

/// What a row inside a reply is, going by where its text starts and what it
/// starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowKind {
    /// Text from the left edge of the reply's area. A row drawn right to
    /// left may start one column in: see the Undo step.
    Text,
    /// Text that starts further in than a paragraph's rows do.
    Deep,
    ListItem,
    Table,
    ToolResult,
    /// A line of a quotation, after the bar the app draws before it.
    Quotation,
}

fn row_kind(row: &[Cell]) -> RowKind {
    let text = &row[TEXT_COL.min(row.len())..];
    let Some(start) = text.iter().position(|cell| !is_empty(cell)) else {
        return RowKind::Text;
    };
    let first = text[start].character;
    let then_empty = |at: usize| text.get(at).is_none_or(is_empty);
    // List items are indented two columns for each level.
    let list_marker = || {
        if start % 2 != 0 {
            return false;
        }
        if matches!(first, '-' | '*' | '+' | '•') {
            return then_empty(start + 1);
        }
        let digits = text[start..]
            .iter()
            .take_while(|cell| cell.character.is_ascii_digit())
            .count();
        digits > 0
            && text
                .get(start + digits)
                .is_some_and(|cell| matches!(cell.character, '.' | ')'))
            && then_empty(start + digits + 1)
    };
    if ('\u{2500}'..='\u{257f}').contains(&first) {
        RowKind::Table
    } else if first == TOOL_RESULT_MARK {
        RowKind::ToolResult
    } else if first == QUOTATION_BAR {
        RowKind::Quotation
    } else if list_marker() {
        RowKind::ListItem
    } else if start <= 1 {
        RowKind::Text
    } else {
        RowKind::Deep
    }
}

/// True when a cell of the row has one of the Terminal's numbered colours,
/// which the app gives to the words of a code block: the keywords, numbers,
/// strings, comments and built-in names of a language it knows. A link in a
/// paragraph has such a colour too, and is told by being a link.
fn holds_code_colour(row: &[Cell]) -> bool {
    row.iter().any(|cell| {
        !cell.is_left_alone()
            && matches!(cell.style.foreground, Color::Palette(_))
            && cell.style.link.is_none()
    })
}

/// True when every character of the row's text is drawn in a colour, or
/// every one is drawn dim. The app draws a code block with no language
/// named wholly in the colour it gives inline code, and above the code of a
/// language it does not know it draws the language's name, dim. In a
/// paragraph, the words between such words are in the Terminal's own
/// colours.
fn is_wholly_coloured_or_dim(row: &[Cell]) -> bool {
    let mut characters = row[TEXT_COL.min(row.len())..]
        .iter()
        .filter(|cell| !is_empty(cell))
        .peekable();
    characters.peek().is_some()
        && (characters
            .clone()
            .all(|cell| cell.style.foreground != Color::Default)
            || characters.all(|cell| cell.style.dim))
}

/// True when the app, wrapping one text in an area `width` cells wide,
/// would have broken it between `row` and `next`: the first word of `next`
/// did not fit after the last word of `row`.
fn wraps_into(row: &[Cell], next: &[Cell], width: usize) -> bool {
    let text = |row: &[Cell]| -> Vec<char> {
        let cells = &row[TEXT_COL.min(row.len())..];
        let start = cells.iter().position(|cell| !is_empty(cell)).unwrap_or(0);
        let end = cells
            .iter()
            .rposition(|cell| !is_empty(cell))
            .map_or(start, |last| last + 1);
        cells[start..end]
            .iter()
            .map(|cell| match cell.is_left_alone() {
                true => ' ',
                false => cell.character,
            })
            .collect()
    };
    let (row, next) = (text(row), text(next));
    row.len() + 1 + first_word_len(&next) > width
}

/// The length of the word a row's text begins with. The app draws a row
/// that begins with a Farsi letter from the right, so the word is at
/// whichever end the row's letters say. A row with an English word at its
/// left end and Farsi at its right end may begin at either (see the Undo
/// step). For such a row the longer of the two words is taken: the row
/// above is then counted as wrapped when either word would not have fitted
/// on it, which is far more often a wrapped paragraph than two lines.
fn first_word_len(text: &[char]) -> usize {
    let leftmost = text.iter().take_while(|c| **c != ' ').count();
    let rightmost = text.iter().rev().take_while(|c| **c != ' ').count();
    let mut letters = text.iter().filter_map(|c| match layout::bearing(*c) {
        Bearing::Left => Some(false),
        Bearing::Right => Some(true),
        _ => None,
    });
    let first_is_farsi = letters.next();
    let last_is_farsi = letters.next_back().or(first_is_farsi);
    match (first_is_farsi, last_is_farsi) {
        (Some(true), Some(true)) => rightmost,
        (Some(false), Some(true)) => leftmost.max(rightmost),
        _ => leftmost,
    }
}

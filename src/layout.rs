//! Layout: takes a paragraph in Logical order and the width it was drawn in,
//! and returns the cells in Screen order together with a map between Logical
//! and Screen positions.
//!
//! It holds no state and knows nothing about Terminals or Target apps.
//!
//! The order is that of the Unicode reordering rules (`reorder.rs`), with a
//! few rules of farcli's own where those rules alone do not give what a
//! Farsi reader wants. Each is in one place, so that it can be found and
//! replaced:
//!
//! - which way a paragraph runs, and a paragraph or list item with no
//!   letter in it (`direction.rs`);
//! - numbers joined by a mark keep the order they were written in
//!   (`numbers.rs`);
//! - a word with no Farsi in it stays in one piece (`words.rs`).

mod brackets;
mod char_class;
mod direction;
mod joining;
mod numbers;
mod reorder;
mod words;

use crate::screen::{Cell, CellWidth, Row};

use char_class::{class_of, CharClass};
use joining::Letter;

pub(crate) use direction::direction_of_an_item_with_no_letter;
pub use direction::paragraph_direction;

/// The character that ends a letter's join without leaving a gap. It takes
/// no cell: the Hidden screen keeps it with the letter before it.
const HALF_SPACE: char = '\u{200c}';

/// Which way a paragraph, or a run of text inside it, is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
}

/// One line of a paragraph, laid out for the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The whole line in Screen order: as many cells as the width it was
    /// laid out in.
    pub cells: Row,
    /// For each Logical position, the column its cell is shown in.
    screen_cols: Vec<usize>,
}

impl Line {
    /// The column that shows the cell at the Logical position `logical`.
    /// Positions after the end of the text are part of the map too: they
    /// are the empty cells the text would grow into.
    pub fn screen_col(&self, logical: usize) -> usize {
        self.screen_cols.get(logical).copied().unwrap_or(logical)
    }

    /// The Logical position of the cell shown in column `screen`.
    pub fn logical_col(&self, screen: usize) -> usize {
        self.screen_cols
            .iter()
            .position(|col| *col == screen)
            .unwrap_or(screen)
    }
}

/// True when the cells hold a letter or digit of a right-to-left script.
/// Laying out cells that hold none changes nothing.
pub fn holds_right_to_left(cells: &[Cell]) -> bool {
    cells.iter().any(|cell| {
        matches!(
            class_of(cell.character),
            CharClass::Right | CharClass::Arabic | CharClass::ArabicDigit
        ) || matches!(cell.character, '۰'..='۹')
    })
}

/// True when the cells hold a letter, of whatever script: a text that
/// holds one takes its direction from its first.
pub(crate) fn holds_a_letter(cells: &[Cell]) -> bool {
    direction::direction_of_first_letter(cells).is_some()
}

/// How a character bears on the order of the text around it, for code
/// outside Layout that has to reason about order: the Undo step, which works
/// back from what a Target app drew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bearing {
    /// A letter of a left-to-right script.
    Left,
    /// A letter of a right-to-left script.
    Right,
    Digit,
    /// A space, punctuation or anything else that takes its direction from
    /// its neighbours.
    Weak,
}

pub(crate) fn bearing(character: char) -> Bearing {
    match class_of(character) {
        CharClass::Left => Bearing::Left,
        CharClass::Right | CharClass::Arabic => Bearing::Right,
        CharClass::Digit | CharClass::ArabicDigit => Bearing::Digit,
        _ => Bearing::Weak,
    }
}

/// A character that is written inside or beside a number, and by the
/// reordering rules can count as part of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumberMark {
    /// A plus or minus sign: part of a number when it stands alone between
    /// two digits.
    Sign,
    /// A point, comma, colon or slash: part of a number when it stands
    /// alone between two digits.
    Joiner,
    /// A sign such as % or $: part of a number it stands beside.
    End,
}

pub(crate) fn number_mark(character: char) -> Option<NumberMark> {
    match class_of(character) {
        CharClass::Sign => Some(NumberMark::Sign),
        CharClass::NumberJoiner => Some(NumberMark::Joiner),
        CharClass::NumberEnd => Some(NumberMark::End),
        _ => None,
    }
}

/// True for a character that takes no cell of its own and is drawn with the
/// character before it: a Half-space, or an accent or vowel mark.
pub(crate) fn takes_no_cell(character: char) -> bool {
    matches!(character, HALF_SPACE | '\u{200d}') || class_of(character) == CharClass::Mark
}

/// The Logical positions of a line's characters in the order they sit on
/// screen from the left, by the Unicode reordering rules alone, for a line
/// whose direction is `direction`. This is the order a Target app that
/// reorders Farsi itself draws: it knows nothing of the rules Layout adds.
pub(crate) fn screen_order(characters: &[char], direction: Direction) -> Vec<usize> {
    reorder::screen_order(&reorder::levels(characters, direction, &[]))
}

/// The stretches of a line that Layout's own rules keep in one piece, read
/// left to right: numbers joined by a mark, and words with no Farsi in
/// them. Each rule is in a file of its own.
fn pieces_read_left_to_right(characters: &[char]) -> Vec<std::ops::Range<usize>> {
    let mut pieces = numbers::joined_numbers(characters);
    pieces.extend(words::whole_words(characters));
    pieces
}

/// Lays out one line of a paragraph whose Paragraph direction is `direction`.
///
/// `logical` is the line's text in Logical order, one cell per character
/// (two for a character two cells wide), and no longer than `width`. The
/// result is `width` cells long: a right-to-left line ends at the right
/// edge.
pub fn lay_out_line(logical: &[Cell], direction: Direction, width: usize) -> Line {
    let mut cells: Vec<Cell> = logical.to_vec();
    cells.resize(width.max(logical.len()), Cell::blank());

    let letters: Vec<Letter> = cells
        .iter()
        .map(|cell| Letter {
            character: cell.character,
            half_space_after: cell.marks.contains(&HALF_SPACE),
        })
        .collect();
    let joined = joining::joined(&letters);

    let units = units(&cells);
    let characters: Vec<char> = units
        .iter()
        .map(|unit| cells[unit.first].character)
        .collect();
    let levels = reorder::levels(
        &characters,
        direction,
        &pieces_read_left_to_right(&characters),
    );
    let order = reorder::screen_order(&levels);

    let mut screen = Vec::with_capacity(cells.len());
    let mut screen_cols = vec![0; cells.len()];
    for index in order {
        let unit = &units[index];
        let right_to_left = levels[index] % 2 == 1;
        for position in unit.first..unit.first + unit.len {
            screen_cols[position] = screen.len();
            let mut cell = cells[position].clone();
            cell.character = if right_to_left {
                brackets::mirrored(joined[position])
            } else {
                joined[position]
            };
            // The Half-space has done its work on the join and is not sent.
            cell.marks.retain(|mark| *mark != HALF_SPACE);
            screen.push(cell);
        }
    }
    Line {
        cells: screen,
        screen_cols,
    }
}

/// The cells of one character: two for a character two cells wide, which
/// must stay together and in order wherever it is moved.
struct Unit {
    /// The Logical position of the character's first cell.
    first: usize,
    len: usize,
}

fn units(cells: &[Cell]) -> Vec<Unit> {
    let mut units: Vec<Unit> = Vec::with_capacity(cells.len());
    for (position, cell) in cells.iter().enumerate() {
        match units.last_mut() {
            Some(unit) if cell.width == CellWidth::SecondHalf && unit.len == 1 => unit.len = 2,
            _ => units.push(Unit {
                first: position,
                len: 1,
            }),
        }
    }
    units
}

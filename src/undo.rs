//! Undo step: for a Target app that reorders Farsi itself (Claude Code on
//! Windows), turns its rows back into Logical order before layout.
//!
//! Claude Code puts each row into Screen order on its own, by the Unicode
//! reordering rules, with the row's direction taken from the row's first
//! letter. It turns no bracket round and joins no letter. The step works
//! back from the cells on screen to an order that those rules turn into
//! exactly what is on screen, and checks every answer by running the rules
//! forward again. A row for which no order checks out has no reading, and
//! whoever asked leaves it as the app drew it.
//!
//! The screen cannot always settle the order. Three things can be in doubt:
//!
//! 1. Which way the row was drawn. A row with an English word at its left
//!    end and Farsi at its right end is what the app draws both for a row
//!    that begins with the English word and for one that ends with it.
//! 2. Where a number belongs. In a row drawn right to left, `x 20 33` is
//!    what the app draws for «33 x 20», for «x 20 33» and for «33 20 x».
//! 3. Which brackets belong together, when brackets stand between two
//!    stretches of Farsi. This is rare in text as people write it.
//!
//! For those rows the step guesses the likelier reading from the paragraph
//! the row is in. Every rule of the guess is in this file: `drawn_direction`
//! for the first doubt, and for the other two the order in which `reading`
//! tries its answers and `oddness_of_brackets`.
//!
//! The doubts only show when a row was drawn against its paragraph's
//! direction, as a wrapped row that begins with an English word is in a
//! Farsi paragraph. A row drawn the way its paragraph runs is laid out
//! with every character where the app had it, whichever reading is taken,
//! because Layout follows the same reordering rules.

use crate::layout::{self, Bearing, Direction};
use crate::screen::{Cell, CellWidth};

/// One row of a paragraph as the app drew it.
#[derive(Debug, Clone)]
pub(crate) struct DrawnRow {
    /// The cells from the left edge of the area the paragraph was drawn in
    /// up to the row's last character.
    pub cells: Vec<Cell>,
    /// The app wrapped the paragraph after this row: the next row carries
    /// on the same text.
    pub wrapped: bool,
}

/// Where the paragraph begins, as far as the screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Start {
    /// The first row given is the paragraph's first row.
    Here,
    /// The paragraph began above the first row given. Its direction is
    /// known when its first row was seen earlier.
    Above(Option<Direction>),
}

/// A paragraph in Logical order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Paragraph {
    /// The Paragraph direction: that of the paragraph's first letter.
    pub direction: Direction,
    /// Each row's cells in Logical order, or `None` for a row with no
    /// reading.
    pub rows: Vec<Option<Vec<Cell>>>,
}

/// Turns the rows of one paragraph back into Logical order. `width` is the
/// width of the area the app wrapped the paragraph in.
pub(crate) fn paragraph(rows: &[DrawnRow], width: usize, start: Start) -> Paragraph {
    let rows: Vec<Row> = rows.iter().map(|row| Row::read(row, width)).collect();

    let direction = match start {
        Start::Above(Some(direction)) => direction,
        // With its first rows out of sight, a paragraph that holds a row
        // certainly drawn right to left is far more often a Farsi paragraph
        // than an English one with Farsi in it.
        Start::Above(None) if rows.iter().any(Row::certainly_right_to_left) => {
            Direction::RightToLeft
        }
        // The app takes a row's direction from the row's first letter, so
        // the direction it drew the paragraph's first letters in is the
        // Paragraph direction.
        _ => rows
            .iter()
            .find(|row| row.holds_a_letter)
            .and_then(|row| row.drawn_direction(None))
            .unwrap_or(Direction::LeftToRight),
    };

    let rows = rows
        .iter()
        .map(|row| {
            let order = match row.drawn_direction(Some(direction))? {
                Direction::LeftToRight => row.left_to_right.as_ref()?,
                Direction::RightToLeft => row.right_to_left.as_ref()?,
            };
            Some(
                order
                    .iter()
                    .flat_map(|unit| row.units[*unit].clone())
                    .map(|cell| row.cells[cell].clone())
                    .collect(),
            )
        })
        .collect();
    Paragraph { direction, rows }
}

/// A drawn row with its readings.
struct Row<'a> {
    cells: &'a [Cell],
    /// The cells of each character, in Screen order.
    units: Vec<std::ops::Range<usize>>,
    characters: Vec<char>,
    holds_a_letter: bool,
    /// The row's characters in Logical order if the app drew the row left
    /// to right, as positions in `units`. `None` when it cannot have.
    left_to_right: Option<Vec<usize>>,
    /// The same if the app drew the row right to left.
    right_to_left: Option<Vec<usize>>,
    wrapped: bool,
    /// The row's text reaches the right edge of the area.
    fills_the_area: bool,
}

impl<'a> Row<'a> {
    fn read(row: &'a DrawnRow, width: usize) -> Row<'a> {
        let units = units(&row.cells);
        let characters: Vec<char> = units
            .iter()
            .map(|unit| row.cells[unit.start].character)
            .collect();
        Row {
            cells: &row.cells,
            holds_a_letter: characters
                .iter()
                .any(|c| matches!(layout::bearing(*c), Bearing::Left | Bearing::Right)),
            left_to_right: reading(&characters, Direction::LeftToRight),
            right_to_left: reading(&characters, Direction::RightToLeft),
            units,
            characters,
            wrapped: row.wrapped,
            fills_the_area: row.cells.len() >= width,
        }
    }

    fn certainly_right_to_left(&self) -> bool {
        self.right_to_left.is_some() && self.left_to_right.is_none()
    }

    /// Which way the app drew the row. `paragraph` is the Paragraph
    /// direction, when the row is not the one that decides it.
    fn drawn_direction(&self, paragraph: Option<Direction>) -> Option<Direction> {
        match (&self.left_to_right, &self.right_to_left) {
            (None, None) => return None,
            (Some(_), None) => return Some(Direction::LeftToRight),
            (None, Some(_)) => return Some(Direction::RightToLeft),
            (Some(_), Some(_)) => {}
        }
        // The first doubt: the row reads either way. What follows is the
        // guess, strongest evidence first.

        // The app's wrap leaves the space it broke at on the end of the
        // row. Drawn right to left, that space shows on the left of the
        // row's text, and nothing else puts an empty cell there.
        if self.characters.first() == Some(&' ') {
            return Some(Direction::RightToLeft);
        }
        // So a wrapped row with room for that space and no space on its
        // left was drawn left to right.
        if self.wrapped && !self.fills_the_area {
            return Some(Direction::LeftToRight);
        }
        // A mark that closes a sentence or a pair of brackets is not what
        // a row begins with, and a bracket that opens is not what it ends
        // with. Whichever end of the screen row holds one tells where the
        // row's text ends or begins.
        let leftmost = self.characters.iter().find(|c| **c != ' ');
        let rightmost = self.characters.iter().rfind(|c| **c != ' ');
        match (
            leftmost.map(|c| end_mark(*c)),
            rightmost.map(|c| end_mark(*c)),
        ) {
            (_, Some(EndMark::Closes)) | (Some(EndMark::Opens), _) => {
                return Some(Direction::LeftToRight)
            }
            (Some(EndMark::Closes), _) | (_, Some(EndMark::Opens)) => {
                return Some(Direction::RightToLeft)
            }
            _ => {}
        }
        // Nothing on the row tells. Inside a paragraph, a row is most
        // likely drawn the way the paragraph runs. A row that decides the
        // direction itself holds Farsi at its right end, and is taken to
        // begin there.
        Some(paragraph.unwrap_or(Direction::RightToLeft))
    }
}

/// What a character at one end of a row says about that end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndMark {
    /// It closes a sentence, a clause or a pair of brackets: text ends with
    /// it and does not begin with it.
    Closes,
    /// It opens a pair of brackets: text begins with it and does not end
    /// with it.
    Opens,
    Neither,
}

/// The app turns no bracket round, so a bracket on screen is the bracket
/// that was written.
fn end_mark(character: char) -> EndMark {
    match character {
        '.' | ',' | ';' | ':' | '!' | '?' | '،' | '؛' | '؟' | '…' | ')' | ']' | '}' | '»' => {
            EndMark::Closes
        }
        '(' | '[' | '{' | '«' => EndMark::Opens,
        _ => EndMark::Neither,
    }
}

/// The cells of each character of a row: two for a character two cells
/// wide, which moves as one.
fn units(cells: &[Cell]) -> Vec<std::ops::Range<usize>> {
    let mut units: Vec<std::ops::Range<usize>> = Vec::with_capacity(cells.len());
    for (position, cell) in cells.iter().enumerate() {
        match units.last_mut() {
            Some(unit) if cell.width == CellWidth::SecondHalf && unit.len() == 1 => {
                unit.end = position + 1
            }
            _ => units.push(position..position + 1),
        }
    }
    units
}

/// A stretch of a screen row whose characters bear on order the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Piece {
    /// Letters of a left-to-right script.
    Left,
    /// Letters of a right-to-left script.
    Right,
    /// The digits of a number.
    Digits,
    /// A bracket, which the reordering rules may treat as a letter of
    /// either direction so that it stays with its partner.
    Bracket,
    /// Spaces and punctuation, which take their direction from what stands
    /// on either side.
    Gap,
}

/// How a piece is read: as text running left to right, as text running
/// right to left, or as taking its direction from its neighbours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Neutral,
}

/// How many of a row's numbers and brackets are tried both ways. A row with
/// more keeps the likeliest reading of the rest.
const MOST_DOUBTS: usize = 10;

/// The Logical order of a screen row's characters, as positions in the
/// row, if the app drew the row with the direction `drawn`. When more than
/// one order gives the same screen row (the second doubt) this is the
/// likeliest: the first one found, trying the likeliest sides first (see
/// `sides`), in which every bracket has its partner. `None` when no order
/// tried gives the screen row.
fn reading(characters: &[char], drawn: Direction) -> Option<Vec<usize>> {
    // A row drawn left to right has an English letter as its leftmost
    // letter, or no letter at all. A row drawn right to left has a Farsi
    // letter as its rightmost. This settles most rows before any search.
    if !may_be_drawn(characters, drawn) {
        return None;
    }
    let pieces = pieces(characters);
    let doubts: Vec<usize> = (0..pieces.len())
        .filter(|at| matches!(pieces[*at].0, Piece::Digits | Piece::Bracket))
        .take(MOST_DOUBTS)
        .collect();

    // Likeliest first: every doubt read the likeliest way, then one read
    // another way, then two.
    let mut other_ways: Vec<Vec<(usize, Side)>> = vec![Vec::new()];
    const WAYS: [Side; 2] = [Side::Left, Side::Right];
    for (index, first) in doubts.iter().enumerate() {
        for way in WAYS {
            other_ways.push(vec![(*first, way)]);
            for second in &doubts[index + 1..] {
                for other in WAYS {
                    other_ways.push(vec![(*first, way), (*second, other)]);
                }
            }
        }
    }
    other_ways.sort_by_key(Vec::len);

    // The first order found whose brackets sit as brackets are written.
    // Failing that, the one whose brackets are least odd.
    let mut least_odd: Option<(usize, Vec<usize>)> = None;
    for told in &other_ways {
        let levels = levels(&pieces, drawn, told);
        let order = logical_order(&levels);
        if !checks_out(characters, &order, drawn) {
            continue;
        }
        let logical: Vec<char> = order.iter().map(|at| characters[*at]).collect();
        match oddness_of_brackets(&logical) {
            0 => return Some(order),
            oddness if least_odd.as_ref().is_none_or(|(least, _)| oddness < *least) => {
                least_odd = Some((oddness, order))
            }
            _ => {}
        }
    }
    least_odd.map(|(_, order)| order)
}

/// How many ways of reading a row's numbers and brackets `only_order`
/// goes through. A row with more is not one it can be sure of.
const MOST_WAYS_SETTLED: usize = 1024;

/// The Logical order of a screen row's characters, as positions in the row,
/// when the screen settles it: when one order, and no other, is drawn as
/// this row. `None` for a row with any of the three doubts, and for a row
/// no order gives.
///
/// This is for text nobody may guess at: what the user is typing in the
/// Input box, where a wrong order puts the cursor on the wrong letter.
pub(crate) fn only_order(characters: &[char]) -> Option<Vec<usize>> {
    let pieces = pieces(characters);
    let doubts: Vec<usize> = (0..pieces.len())
        .filter(|at| matches!(pieces[*at].0, Piece::Digits | Piece::Bracket))
        .collect();
    // Every way of reading every number and bracket: a number with the text
    // on one side of it or the other, a bracket also as taking its
    // direction from its neighbours.
    let ways = |piece: usize| -> &'static [Side] {
        match pieces[piece].0 {
            Piece::Bracket => &[Side::Neutral, Side::Left, Side::Right],
            _ => &[Side::Left, Side::Right],
        }
    };
    let mut all_ways = 1usize;
    for piece in &doubts {
        all_ways = all_ways.saturating_mul(ways(*piece).len());
    }
    if all_ways > MOST_WAYS_SETTLED {
        return None;
    }
    let mut found: Option<Vec<usize>> = None;
    for drawn in [Direction::LeftToRight, Direction::RightToLeft] {
        if !may_be_drawn(characters, drawn) {
            continue;
        }
        let mut chosen = vec![0; doubts.len()];
        loop {
            let told: Vec<(usize, Side)> = doubts
                .iter()
                .zip(&chosen)
                .map(|(piece, way)| (*piece, ways(*piece)[*way]))
                .collect();
            let order = logical_order(&levels(&pieces, drawn, &told));
            if checks_out(characters, &order, drawn) {
                match &found {
                    Some(other) if *other != order => return None,
                    _ => found = Some(order),
                }
            }
            // The next way of reading the doubts, counting up.
            let mut at = 0;
            while at < doubts.len() && chosen[at] + 1 == ways(doubts[at]).len() {
                chosen[at] = 0;
                at += 1;
            }
            match chosen.get_mut(at) {
                Some(way) => *way += 1,
                None => break,
            }
        }
    }
    found
}

/// A row drawn left to right has an English letter as its leftmost letter,
/// or no letter at all. A row drawn right to left has a Farsi letter as its
/// rightmost.
fn may_be_drawn(characters: &[char], drawn: Direction) -> bool {
    let is_letter = |c: &&char| matches!(layout::bearing(**c), Bearing::Left | Bearing::Right);
    match drawn {
        Direction::LeftToRight => characters
            .iter()
            .find(is_letter)
            .is_none_or(|c| layout::bearing(*c) == Bearing::Left),
        Direction::RightToLeft => characters
            .iter()
            .rfind(is_letter)
            .is_some_and(|c| layout::bearing(*c) == Bearing::Right),
    }
}

/// How far the brackets of a text are from the way brackets are written:
/// in pairs, the opening one against the first word it encloses and the
/// closing one against the last. Zero when they are written so. A bracket
/// with no partner counts for more than any number of misplaced spaces.
///
/// More than one order of the same row can check out when brackets stand
/// between two stretches of Farsi, and this is what tells them apart.
fn oddness_of_brackets(text: &[char]) -> usize {
    const NO_PARTNER: usize = 100;
    let mut oddness = 0;
    let mut open = Vec::new();
    for (at, character) in text.iter().enumerate() {
        let before = at.checked_sub(1).map(|at| text[at]);
        let after = text.get(at + 1).copied();
        match character {
            '(' | '[' | '{' | '«' => {
                open.push(match character {
                    '(' => ')',
                    '[' => ']',
                    '{' => '}',
                    _ => '»',
                });
                if after.is_none_or(|c| c == ' ') {
                    oddness += 1;
                }
            }
            ')' | ']' | '}' | '»' => {
                if open.pop() != Some(*character) {
                    oddness += NO_PARTNER;
                }
                if before.is_none_or(|c| c == ' ') {
                    oddness += 1;
                }
            }
            _ => {}
        }
    }
    oddness + open.len() * NO_PARTNER
}

/// A screen row cut into pieces, each with how many characters it holds.
fn pieces(characters: &[char]) -> Vec<(Piece, usize)> {
    let mut pieces: Vec<(Piece, usize)> = Vec::new();
    for character in characters {
        let piece = match layout::bearing(*character) {
            Bearing::Left => Piece::Left,
            Bearing::Right => Piece::Right,
            Bearing::Digit => Piece::Digits,
            Bearing::Weak if matches!(character, '(' | ')' | '[' | ']' | '{' | '}') => {
                Piece::Bracket
            }
            Bearing::Weak => Piece::Gap,
        };
        match pieces.last_mut() {
            Some((last, len)) if *last == piece && piece != Piece::Bracket => *len += 1,
            _ => pieces.push((piece, 1)),
        }
    }
    pieces
}

/// The side each piece of a screen row is read on. `told` names pieces to
/// read another way than the likeliest.
///
/// The likeliest way, which is the guess for the second doubt:
/// - A number that stands right after an English word, as in `Python 3`,
///   belongs to that word and reads left to right with it. Any other number
///   is a number in the Farsi text and counts with the right-to-left text.
/// - A bracket takes its direction from its neighbours, like a space.
fn sides(pieces: &[(Piece, usize)], drawn: Direction, told: &[(usize, Side)]) -> Vec<Side> {
    let edge = match drawn {
        Direction::LeftToRight => Side::Left,
        Direction::RightToLeft => Side::Right,
    };
    let mut sides = Vec::with_capacity(pieces.len());
    // The side of the nearest piece to the left that has one.
    let mut before = edge;
    for (at, (piece, _)) in pieces.iter().enumerate() {
        let told = told
            .iter()
            .find(|(piece, _)| *piece == at)
            .map(|(_, way)| *way);
        let side = match piece {
            Piece::Left => Side::Left,
            Piece::Right => Side::Right,
            Piece::Gap => Side::Neutral,
            Piece::Bracket => told.unwrap_or(Side::Neutral),
            Piece::Digits => told.unwrap_or(before),
        };
        if side != Side::Neutral {
            before = side;
        }
        sides.push(side);
    }
    sides
}

/// How deep each character of a screen row sits in runs of text that
/// alternate in direction, if the row was drawn with the direction `drawn`
/// and its pieces are read as `told`. These are the levels of the Unicode
/// reordering rules, worked out from the screen instead of from Logical
/// order.
fn levels(pieces: &[(Piece, usize)], drawn: Direction, told: &[(usize, Side)]) -> Vec<u8> {
    let sides = sides(pieces, drawn, told);
    // The level of the row itself, of text running against it, and the
    // side that text is on.
    let (base, against_level, against) = match drawn {
        Direction::LeftToRight => (0, 1, Side::Right),
        Direction::RightToLeft => (1, 2, Side::Left),
    };
    let edge = match drawn {
        Direction::LeftToRight => Side::Left,
        Direction::RightToLeft => Side::Right,
    };
    let side_before = |at: usize| {
        sides[..at]
            .iter()
            .rev()
            .find(|side| **side != Side::Neutral)
            .copied()
            .unwrap_or(edge)
    };
    let side_after = |at: usize| {
        sides[at + 1..]
            .iter()
            .find(|side| **side != Side::Neutral)
            .copied()
            .unwrap_or(edge)
    };

    let mut levels = Vec::new();
    for (at, (piece, len)) in pieces.iter().enumerate() {
        let level = match (piece, sides[at]) {
            // A number sits one level deeper than right-to-left text. Read
            // with an English word it is part of that word.
            (_, Side::Left) => base * 2,
            (Piece::Digits, _) => 2,
            (_, Side::Right) => 1,
            // Between two pieces that both run against the row, a space or
            // a mark runs with them. Anywhere else it runs with the row.
            (_, Side::Neutral) => {
                if side_before(at) == against && side_after(at) == against {
                    against_level
                } else {
                    base
                }
            }
        };
        levels.extend(std::iter::repeat_n(level, *len));
    }
    levels
}

/// The positions of a screen row's characters in Logical order, given each
/// character's level. This is the Unicode rule that turns runs round (L2),
/// done backwards: from the shallowest right-to-left level down to the
/// deepest.
fn logical_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let highest = levels.iter().copied().max().unwrap_or(0);
    let Some(lowest_odd) = levels.iter().copied().filter(|level| level % 2 == 1).min() else {
        return order;
    };
    for level in lowest_odd..=highest {
        let mut start = 0;
        while start < order.len() {
            if levels[order[start]] < level {
                start += 1;
                continue;
            }
            let end = (start..order.len())
                .find(|at| levels[order[*at]] < level)
                .unwrap_or(order.len());
            order[start..end].reverse();
            start = end;
        }
    }
    order
}

/// True when the app, given the row in the Logical order `order`, would
/// have drawn it with the direction `drawn` and put every character where
/// it is on screen.
fn checks_out(characters: &[char], order: &[usize], drawn: Direction) -> bool {
    let logical: Vec<char> = order.iter().map(|at| characters[*at]).collect();
    // The app takes the row's direction from its first letter, and draws a
    // row with no letter left to right.
    let first_letter = logical.iter().find_map(|c| match layout::bearing(*c) {
        Bearing::Left => Some(Direction::LeftToRight),
        Bearing::Right => Some(Direction::RightToLeft),
        _ => None,
    });
    if first_letter.unwrap_or(Direction::LeftToRight) != drawn {
        return false;
    }
    layout::screen_order(&logical, drawn)
        .iter()
        .enumerate()
        .all(|(on_screen, logical)| order[*logical] == on_screen)
}

//! Reordering: from Logical order into Screen order.
//!
//! This follows the Unicode reordering rules (Unicode Standard Annex 9) for
//! one line that holds no direction marks. The rule names in the comments
//! (W1, N1, L2 and so on) are that document's.

use std::ops::Range;

use super::brackets;
use super::char_class::{class_of, CharClass};
use super::Direction;

/// How deep a character sits in runs of text that alternate in direction.
/// An even level reads left to right and an odd one right to left.
pub(super) type Level = u8;

/// The level of each character of a line, given the line in Logical order
/// and the Paragraph direction.
///
/// `left_to_right` names stretches of the line that are read left to right
/// as one piece whatever stands in them: Layout's own rules pick them, and
/// the Unicode rules take every character of such a stretch for a letter of
/// a left-to-right script. With none named, the levels are those of the
/// Unicode rules alone.
pub(super) fn levels(
    characters: &[char],
    direction: Direction,
    left_to_right: &[Range<usize>],
) -> Vec<Level> {
    use CharClass::*;

    let classes: Vec<CharClass> = characters.iter().map(|c| class_of(*c)).collect();

    let base: Level = match direction {
        Direction::LeftToRight => 0,
        Direction::RightToLeft => 1,
    };
    // What the text counts as having before its start and after its end.
    let edge = match direction {
        Direction::LeftToRight => Left,
        Direction::RightToLeft => Right,
    };
    let mut kinds = classes.to_vec();
    let len = kinds.len();

    // W1: a mark is whatever the character before it is.
    let mut before = edge;
    for kind in &mut kinds {
        if *kind == Mark {
            *kind = before;
        }
        before = *kind;
    }

    // W2: digits that follow Arabic-script letters behave as Arabic digits.
    let mut last_letter = edge;
    for kind in &mut kinds {
        match *kind {
            Left | Right | Arabic => last_letter = *kind,
            Digit if last_letter == Arabic => *kind = ArabicDigit,
            _ => {}
        }
    }

    // W3: from here on an Arabic-script letter is any right-to-left letter.
    for kind in &mut kinds {
        if *kind == Arabic {
            *kind = Right;
        }
    }

    // W4: one sign or joiner between two digits is part of the number.
    for index in 1..len.saturating_sub(1) {
        kinds[index] = match (kinds[index - 1], kinds[index], kinds[index + 1]) {
            (Digit, Sign | NumberJoiner, Digit) => Digit,
            (ArabicDigit, NumberJoiner, ArabicDigit) => ArabicDigit,
            (_, unchanged, _) => unchanged,
        };
    }

    // W5: signs such as % next to a number are part of the number.
    let mut index = 0;
    while index < len {
        if kinds[index] != NumberEnd {
            index += 1;
            continue;
        }
        let end = (index..len)
            .find(|at| kinds[*at] != NumberEnd)
            .unwrap_or(len);
        let digit_before = index > 0 && kinds[index - 1] == Digit;
        let digit_after = end < len && kinds[end] == Digit;
        if digit_before || digit_after {
            kinds[index..end].fill(Digit);
        }
        index = end;
    }

    // W6: any other sign or joiner is ordinary punctuation.
    for kind in &mut kinds {
        if matches!(*kind, Sign | NumberEnd | NumberJoiner) {
            *kind = Neutral;
        }
    }

    // W7: digits that follow left-to-right letters read as part of them.
    let mut last_letter = edge;
    for kind in &mut kinds {
        match *kind {
            Left | Right => last_letter = *kind,
            Digit if last_letter == Left => *kind = Left,
            _ => {}
        }
    }

    for piece in left_to_right {
        kinds[piece.clone()].fill(Left);
    }

    // From here on a number counts as right-to-left text.
    let side = |kind: CharClass| match kind {
        Left => Some(Left),
        Right | Digit | ArabicDigit => Some(Right),
        _ => None,
    };

    // N0: a pair of brackets takes one direction for both, so that it stays
    // a pair. That is the Paragraph direction when the pair encloses text
    // running that way. When it encloses only text running the other way,
    // it goes with that text if the text before the pair runs that way too.
    for (opening, closing) in brackets::enclosing_pairs(characters, |at| kinds[at] == Neutral) {
        let mut inside = kinds[opening + 1..closing]
            .iter()
            .filter_map(|kind| side(*kind));
        let pair_side = if inside.clone().any(|inner| inner == edge) {
            edge
        } else if let Some(against) = inside.next() {
            let before = kinds[..opening].iter().rev().find_map(|kind| side(*kind));
            if before == Some(against) {
                against
            } else {
                edge
            }
        } else {
            continue;
        };
        kinds[opening] = pair_side;
        kinds[closing] = pair_side;
    }

    // N1 and N2: spaces and punctuation between two runs of the same
    // direction take that direction. Anywhere else they take the Paragraph
    // direction.
    let side = |kind: CharClass| side(kind).unwrap_or(edge);
    let mut index = 0;
    while index < len {
        if !matches!(kinds[index], Space | Neutral) {
            index += 1;
            continue;
        }
        let end = (index..len)
            .find(|at| !matches!(kinds[*at], Space | Neutral))
            .unwrap_or(len);
        let before = if index > 0 {
            side(kinds[index - 1])
        } else {
            edge
        };
        let after = if end < len { side(kinds[end]) } else { edge };
        kinds[index..end].fill(if before == after { before } else { edge });
        index = end;
    }

    // I1 and I2: a run against the Paragraph direction sits one level
    // deeper, and a number in right-to-left text one deeper again.
    let mut levels: Vec<Level> = kinds
        .iter()
        .map(|kind| match (kind, direction) {
            (Left, Direction::LeftToRight) => 0,
            (Right, _) => 1,
            _ => 2,
        })
        .collect();

    // L1: spaces at the end of the line belong to the paragraph, not to the
    // last word.
    let text_end = classes
        .iter()
        .rposition(|class| *class != Space)
        .map_or(0, |last| last + 1);
    levels[text_end..].fill(base);

    levels
}

/// The Logical positions of a line's characters in the order they sit on
/// screen from the left, given each character's level.
pub(super) fn screen_order(levels: &[Level]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let highest = levels.iter().copied().max().unwrap_or(0);
    let Some(lowest_odd) = levels.iter().copied().filter(|level| level % 2 == 1).min() else {
        return order;
    };

    // L2: from the deepest level up, turn round every run of characters
    // that are at that level or deeper.
    for level in (lowest_odd..=highest).rev() {
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

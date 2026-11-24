//! The rule that numbers joined by a mark keep the order they were written
//! in. It is farcli's own, not one of the Unicode reordering rules, and is
//! kept in this one place so that it can be replaced.
//!
//! By the Unicode rules alone, in right-to-left text «33 x 20» is shown as
//! `x 20 33`, «33 × 20» as `20 × 33` and «2026-11-23» as `23-11-2026`. A
//! reader takes such a thing in as one piece, from the left, whatever
//! language is around it.

use std::ops::Range;

use super::char_class::{class_of, CharClass};

/// The marks that stand between the numbers of a size.
const TIMES: [char; 4] = ['x', 'X', '×', '*'];

/// The stretches of a line that are numbers joined by a mark, each to be
/// read left to right as one piece:
///
/// - a size, which is numbers with a multiplication mark between them, with
///   or without spaces: `33 x 20`, `33×20`, `128 x 32 x 3`;
/// - numbers with a plus or minus sign between them and no space, such as a
///   date: `2026-11-23`.
///
/// Numbers with only spaces, commas or words between them are not joined:
/// «از 10 تا 20» is read from the right like the words around it.
pub(super) fn joined_numbers(characters: &[char]) -> Vec<Range<usize>> {
    let is_digit = |at: usize| {
        characters.get(at).is_some_and(|character| {
            matches!(
                class_of(*character),
                CharClass::Digit | CharClass::ArabicDigit
            )
        })
    };
    // A number: digits, and the points, commas, colons and slashes that sit
    // between two digits.
    let class_at = |at: usize| characters.get(at).map(|character| class_of(*character));
    let number_end = |start: usize| {
        let mut end = start;
        loop {
            if is_digit(end) {
                end += 1;
            } else if end > start
                && class_at(end) == Some(CharClass::NumberJoiner)
                && is_digit(end + 1)
            {
                end += 2;
            } else {
                break end;
            }
        }
    };
    let is_letter = |at: usize| {
        characters
            .get(at)
            .is_some_and(|character| character.is_alphabetic())
    };
    // Where the next number starts, if the number that ends at `end` is
    // joined to one.
    let joined_to = |end: usize| {
        let spaces = |from: usize| {
            (from..characters.len())
                .find(|at| characters[*at] != ' ')
                .unwrap_or(characters.len())
        };
        if class_at(end) == Some(CharClass::Sign) && is_digit(end + 1) {
            return Some(end + 1);
        }
        let mark = spaces(end);
        let next = spaces(mark + 1);
        let stands_alone = !is_letter(mark + 1);
        (characters.get(mark).is_some_and(|c| TIMES.contains(c)) && stands_alone && is_digit(next))
            .then_some(next)
    };

    let mut pieces = Vec::new();
    let mut at = 0;
    while at < characters.len() {
        // A number that is the end of a word, as in `v2`, is not the start
        // of anything.
        if !is_digit(at) {
            at += 1;
            continue;
        }
        let mut end = number_end(at);
        if at > 0 && class_at(at - 1) == Some(CharClass::Left) {
            at = end;
            continue;
        }
        let mut numbers = 1;
        while let Some(next) = joined_to(end) {
            end = number_end(next);
            numbers += 1;
        }
        if numbers > 1 {
            pieces.push(at..end);
        }
        at = end;
    }
    pieces
}

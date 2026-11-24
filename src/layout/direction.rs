//! The Paragraph direction rule. It is known to be imperfect and is kept in
//! this one place so that it can be replaced.

use super::char_class::{class_of, CharClass};
use super::Direction;
use crate::screen::Cell;

/// Paragraph direction comes from the paragraph's first letter: right to
/// left when that is a Farsi letter, left to right otherwise.
///
/// Code, file paths and commands are always left to right. Telling them
/// apart from prose needs to know what the Target app drew, so whoever calls
/// Layout decides that and does not ask here.
pub fn paragraph_direction(paragraph: &[Cell]) -> Direction {
    direction_of_first_letter(paragraph).unwrap_or_else(|| direction_with_no_letter(paragraph))
}

/// The direction the first letter of a text gives, when the text holds a
/// letter.
pub(crate) fn direction_of_first_letter(text: &[Cell]) -> Option<Direction> {
    text.iter().find_map(|cell| match class_of(cell.character) {
        CharClass::Left => Some(Direction::LeftToRight),
        CharClass::Right | CharClass::Arabic => Some(Direction::RightToLeft),
        _ => None,
    })
}

/// The rule for a paragraph that holds no letter, such as a number that
/// stands alone as a label: «۳.» is right to left, because its digits are
/// written as Farsi writes them, and `3.` is left to right. Kept apart so
/// that it can be replaced.
fn direction_with_no_letter(paragraph: &[Cell]) -> Direction {
    let farsi_digit = |cell: &Cell| matches!(cell.character, '۰'..='۹' | '٠'..='٩');
    match paragraph.iter().any(farsi_digit) {
        true => Direction::RightToLeft,
        false => Direction::LeftToRight,
    }
}

/// The rule for a list item that holds no letter, such as «2026»: it runs
/// the way the item before it runs, so that the markers of one list stay
/// on one side. With no item before it, it is a paragraph with no letter.
/// Kept apart so that it can be replaced.
pub(crate) fn direction_of_an_item_with_no_letter(
    item: &[Cell],
    item_before: Option<Direction>,
) -> Direction {
    item_before.unwrap_or_else(|| direction_with_no_letter(item))
}

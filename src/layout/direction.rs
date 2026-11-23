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
    paragraph
        .iter()
        .find_map(|cell| match class_of(cell.character) {
            CharClass::Left => Some(Direction::LeftToRight),
            CharClass::Right | CharClass::Arabic => Some(Direction::RightToLeft),
            _ => None,
        })
        .unwrap_or(Direction::LeftToRight)
}

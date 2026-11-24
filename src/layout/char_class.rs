//! Plumbing: the Unicode table that says which way each character runs.
//! Nothing outside this file names the library that holds it.

use unicode_bidi::{bidi_class, BidiClass};

/// How a character takes part in deciding the direction of the text around
/// it. These are the classes of the Unicode reordering rules, with the ones
/// that cannot sit in a screen cell folded into `Neutral`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CharClass {
    /// A letter of a left-to-right script.
    Left,
    /// A letter of a right-to-left script other than Arabic.
    Right,
    /// A letter of the Arabic script, which Farsi is written in.
    Arabic,
    /// A digit as English and Farsi write it: 0-9 and ۰-۹.
    Digit,
    /// A digit as Arabic writes it: ٠-٩.
    ArabicDigit,
    /// A plus or minus sign.
    Sign,
    /// A sign written next to a number, such as % or $.
    NumberEnd,
    /// A character that sits inside a number, such as a point, comma, colon
    /// or slash.
    NumberJoiner,
    /// An accent or vowel mark, which belongs to the letter before it.
    Mark,
    Space,
    /// Punctuation, brackets, symbols and everything else.
    Neutral,
}

pub(super) fn class_of(character: char) -> CharClass {
    match bidi_class(character) {
        BidiClass::L => CharClass::Left,
        BidiClass::R => CharClass::Right,
        BidiClass::AL => CharClass::Arabic,
        BidiClass::EN => CharClass::Digit,
        BidiClass::AN => CharClass::ArabicDigit,
        BidiClass::ES => CharClass::Sign,
        BidiClass::ET => CharClass::NumberEnd,
        BidiClass::CS => CharClass::NumberJoiner,
        BidiClass::NSM => CharClass::Mark,
        BidiClass::WS => CharClass::Space,
        _ => CharClass::Neutral,
    }
}

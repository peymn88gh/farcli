//! The rule that a word with no Farsi in it stays in one piece. It is
//! farcli's own, not one of the Unicode reordering rules, and is kept in
//! this one place so that it can be replaced.
//!
//! By the Unicode rules alone, the marks at either end of an English word
//! in right-to-left text go with the Farsi around it: `./src/main.rs` is
//! shown as `src/main.rs/.`, `.gitignore` as `gitignore.`, `--help` as
//! `help--` and `main()` as `()main`. Paths, file names, options and code
//! are read left to right, marks and all.

use std::ops::Range;

use super::char_class::{class_of, CharClass};

/// Marks that end a sentence or a clause. At the end of a word they belong
/// to the sentence the word is in.
const ENDS_A_SENTENCE: [char; 10] = ['.', ',', ';', ':', '!', '?', '،', '؛', '؟', '…'];

/// Brackets and quotation marks that open round a word, and those that
/// close round it. They enclose the word and are not part of it.
const OPENS: [char; 7] = ['(', '[', '{', '«', '"', '\'', '`'];
const CLOSES: [char; 7] = [')', ']', '}', '»', '"', '\'', '`'];

/// The stretches of a line that are words with no Farsi in them, each to be
/// read left to right as one piece.
///
/// A word is what stands between two spaces. It counts when it holds an
/// English letter and no Farsi one. The piece is the word without the
/// brackets and quotation marks around it and without the marks that end a
/// sentence after it: in «فایل (main.rs).» the piece is `main.rs`. A
/// bracket that closes one opened inside the word is part of it, as in
/// `main()`.
pub(super) fn whole_words(characters: &[char]) -> Vec<Range<usize>> {
    let mut pieces = Vec::new();
    let mut at = 0;
    while at < characters.len() {
        if class_of(characters[at]) == CharClass::Space {
            at += 1;
            continue;
        }
        let end = (at..characters.len())
            .find(|end| class_of(characters[*end]) == CharClass::Space)
            .unwrap_or(characters.len());
        pieces.extend(piece_of(characters, at..end));
        at = end;
    }
    pieces
}

/// The piece of one word, when the word has one.
fn piece_of(characters: &[char], word: Range<usize>) -> Option<Range<usize>> {
    let classes = || characters[word.clone()].iter().map(|c| class_of(*c));
    if !classes().any(|class| class == CharClass::Left)
        || classes().any(|class| matches!(class, CharClass::Right | CharClass::Arabic))
    {
        return None;
    }
    let start = word.clone().find(|at| !OPENS.contains(&characters[*at]))?;
    let mut end = word
        .clone()
        .rev()
        .find(|at| {
            !ENDS_A_SENTENCE.contains(&characters[*at]) && !CLOSES.contains(&characters[*at])
        })
        .map(|last| last + 1)?;
    // A bracket that closes one opened inside the piece belongs to it.
    let mut open: Vec<char> = Vec::new();
    for character in &characters[start..end] {
        match character {
            '(' => open.push(')'),
            '[' => open.push(']'),
            '{' => open.push('}'),
            ')' | ']' | '}' => {
                open.pop();
            }
            _ => {}
        }
    }
    while end < word.end && open.last() == Some(&characters[end]) {
        open.pop();
        end += 1;
    }
    (start < end).then_some(start..end)
}

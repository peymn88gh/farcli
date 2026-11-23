//! Brackets: the characters that come in pairs facing each other.
//!
//! In right-to-left text the bracket that opens a pair sits on the right, so
//! it has to be drawn as its partner to face the text it encloses. A
//! Terminal does not do that itself.

/// Pairs that face each other, as (the one that faces right, its partner).
const FACING_PAIRS: &[(char, char)] = &[
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('<', '>'),
    ('«', '»'),
    ('‹', '›'),
];

/// The brackets that enclose text, as (opening, closing). Quotation marks
/// and the less-than and greater-than signs face each other but are not
/// matched up as pairs.
const ENCLOSING_PAIRS: &[(char, char)] = &[('(', ')'), ('[', ']'), ('{', '}')];

/// How many brackets can be open at once before matching gives up.
const DEEPEST_NESTING: usize = 63;

/// The positions of each opening bracket and the closing bracket that
/// matches it, ordered by where the pair opens. `may_pair` says whether the
/// character at a position is free to be read as a bracket.
pub(super) fn enclosing_pairs(
    characters: &[char],
    may_pair: impl Fn(usize) -> bool,
) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    // The closing bracket each open bracket waits for, and where it opened.
    let mut open: Vec<(char, usize)> = Vec::new();
    for (position, &character) in characters.iter().enumerate() {
        if !may_pair(position) {
            continue;
        }
        if let Some(&(_, closing)) = ENCLOSING_PAIRS.iter().find(|pair| pair.0 == character) {
            if open.len() == DEEPEST_NESTING {
                break;
            }
            open.push((closing, position));
        } else if let Some(depth) = open.iter().rposition(|(closing, _)| *closing == character) {
            pairs.push((open[depth].1, position));
            open.truncate(depth);
        }
    }
    pairs.sort_unstable();
    pairs
}

/// The same bracket facing the other way, or the character itself when it
/// is not one of a pair.
pub(super) fn mirrored(character: char) -> char {
    FACING_PAIRS
        .iter()
        .find_map(|&(left, right)| match character {
            c if c == left => Some(right),
            c if c == right => Some(left),
            _ => None,
        })
        .unwrap_or(character)
}

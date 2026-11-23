//! Joining: which of its connected forms each Farsi letter is drawn in.
//!
//! A Terminal draws one character per cell and joins nothing itself, so each
//! letter is replaced by the pre-joined form that fits its neighbours.

/// A letter as it comes in Logical order.
#[derive(Debug, Clone, Copy)]
pub(super) struct Letter {
    pub character: char,
    /// A Half-space follows the letter: it does not join the next one.
    pub half_space_after: bool,
}

/// The same characters, with every Farsi letter in the form that joins it to
/// the letters before and after it.
pub(super) fn joined(letters: &[Letter]) -> Vec<char> {
    let joins_next = |index: usize| {
        letters.get(index + 1).is_some_and(|next| {
            let letter = letters[index];
            !letter.half_space_after
                && joins_forward(letter.character)
                && joins_backward(next.character)
        })
    };
    (0..letters.len())
        .map(|index| {
            let character = letters[index].character;
            let Some(forms) = forms(character) else {
                return character;
            };
            let before = index > 0 && joins_next(index - 1);
            let after = joins_next(index);
            match (before, after) {
                (true, true) => forms.middle.unwrap_or(forms.end),
                (true, false) => forms.end,
                (false, true) => forms.start.unwrap_or(forms.alone),
                (false, false) => forms.alone,
            }
        })
        .collect()
}

/// The pre-joined forms of one letter.
struct Forms {
    /// Joined to no letter.
    alone: char,
    /// Joined to the letter before it only.
    end: char,
    /// Joined to the letter after it only. Letters such as alef and dal
    /// never join the letter after them.
    start: Option<char>,
    /// Joined on both sides.
    middle: Option<char>,
}

/// The line that stretches a join (کـــتاب). It joins on both sides and has
/// one form.
const STRETCH: char = '\u{640}';

fn joins_forward(character: char) -> bool {
    character == STRETCH || forms(character).is_some_and(|forms| forms.start.is_some())
}

fn joins_backward(character: char) -> bool {
    character == STRETCH || forms(character).is_some_and(|forms| forms.end != forms.alone)
}

fn forms(letter: char) -> Option<Forms> {
    if let Some(&(_, alone)) = JOIN_BOTH_SIDES.iter().find(|(plain, _)| *plain == letter) {
        return Some(Forms {
            alone,
            end: after(alone, 1),
            start: Some(after(alone, 2)),
            middle: Some(after(alone, 3)),
        });
    }
    if let Some(&(_, alone)) = JOIN_BEFORE_ONLY.iter().find(|(plain, _)| *plain == letter) {
        return Some(Forms {
            alone,
            end: after(alone, 1),
            start: None,
            middle: None,
        });
    }
    // Hamza on its own joins nothing.
    (letter == 'ء').then_some(Forms {
        alone: '\u{fe80}',
        end: '\u{fe80}',
        start: None,
        middle: None,
    })
}

fn after(form: char, steps: u32) -> char {
    char::from_u32(form as u32 + steps).unwrap_or(form)
}

/// Letters that join on both sides, each with its form when it stands alone.
/// Unicode places the end, start and middle forms right after that one, in
/// that order.
const JOIN_BOTH_SIDES: &[(char, char)] = &[
    ('ئ', '\u{fe89}'),
    ('ب', '\u{fe8f}'),
    ('ت', '\u{fe95}'),
    ('ث', '\u{fe99}'),
    ('ج', '\u{fe9d}'),
    ('ح', '\u{fea1}'),
    ('خ', '\u{fea5}'),
    ('س', '\u{feb1}'),
    ('ش', '\u{feb5}'),
    ('ص', '\u{feb9}'),
    ('ض', '\u{febd}'),
    ('ط', '\u{fec1}'),
    ('ظ', '\u{fec5}'),
    ('ع', '\u{fec9}'),
    ('غ', '\u{fecd}'),
    ('ف', '\u{fed1}'),
    ('ق', '\u{fed5}'),
    ('ل', '\u{fedd}'),
    ('م', '\u{fee1}'),
    ('ن', '\u{fee5}'),
    ('ه', '\u{fee9}'),
    // The four letters Farsi adds to the Arabic alphabet, and the Farsi
    // forms of kaf and yeh.
    ('پ', '\u{fb56}'),
    ('چ', '\u{fb7a}'),
    ('ک', '\u{fb8e}'),
    ('گ', '\u{fb92}'),
    ('ی', '\u{fbfc}'),
    // The Arabic kaf and yeh, which Arabic keyboards and older text use.
    ('ك', '\u{fed9}'),
    ('ي', '\u{fef1}'),
];

/// Letters that join the letter before them only, each with its form when it
/// stands alone. Unicode places the end form right after it.
const JOIN_BEFORE_ONLY: &[(char, char)] = &[
    ('آ', '\u{fe81}'),
    ('أ', '\u{fe83}'),
    ('ؤ', '\u{fe85}'),
    ('إ', '\u{fe87}'),
    ('ا', '\u{fe8d}'),
    ('ة', '\u{fe93}'),
    ('د', '\u{fea9}'),
    ('ذ', '\u{feab}'),
    ('ر', '\u{fead}'),
    ('ز', '\u{feaf}'),
    ('ژ', '\u{fb8a}'),
    ('و', '\u{feed}'),
    ('ۀ', '\u{fba4}'),
    // The Arabic dotless yeh ends a word and has no start or middle form
    // among the pre-joined letters used here.
    ('ى', '\u{feef}'),
];

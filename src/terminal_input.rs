//! Makes sense of what the Terminal sends farcli: the keys the user pressed,
//! the text the user pasted, what the mouse did, and the Terminal's answers
//! to what the Target app asked.
//!
//! farcli reads the Terminal the way a hidden terminal reads its own: raw,
//! with keys asked for in Windows' own key report format. What then arrives
//! depends on the console host that sits between the Terminal and farcli.
//!
//! - The one Windows Terminal brings (seen: 1.25) hands on what the Terminal
//!   sent as it sent it: a code for every key going down or up, the text of
//!   a paste as plain characters between the marks `ESC [ 200 ~` and
//!   `ESC [ 201 ~`, and mouse reports such as `ESC [ < 64 ; 10 ; 5 M`.
//! - The older one built into Windows (seen: Windows 11 22H2) turns every
//!   character it hands on into a key code. A code the Terminal sent arrives
//!   spelled out: one key code per character, each a key going down that
//!   has no key number and never comes up.
//!
//! `TerminalInput` reads both, and the hidden terminal is written the same
//! thing for both. This is the one place where farcli learns what the user
//! typed.

use std::io::Write as _;

const ESCAPE: u8 = 0x1b;
const BELL: u8 = 0x07;

/// One thing the Terminal sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Input {
    /// One key going down or coming up.
    Key(Key),
    /// A character that came with no key: pasted text, or text typed in a
    /// Terminal that does not send keys in the Windows form.
    Character(char),
    /// What follows was pasted, up to `PasteEnds`. Only a Target app that
    /// asked the Terminal for these marks is sent them.
    PasteStarts,
    PasteEnds,
    /// The mouse did something the Target app asked to be told about: the
    /// report as the Terminal wrote it.
    Mouse(Vec<u8>),
    /// The Terminal's window gained or lost the keyboard.
    Focus {
        gained: bool,
    },
    /// Anything else, as it came: a key in the form older Terminals send, or
    /// the Terminal's answer to something the Target app asked.
    Other(Vec<u8>),
}

/// A key going down or coming up, with everything Windows says about it. It
/// travels as `ESC [ Vk ; Sc ; Uc ; Kd ; Cs ; Rc _` (win32-input-mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Key {
    /// Windows' number for the key: 37 is Left, 39 is Right. It is 0 when a
    /// character came with no key.
    pub key: u16,
    /// The keyboard's own number for the key.
    pub scan_code: u16,
    /// The character the key types, as one UTF-16 unit, or 0 for none. A
    /// character outside the basic plane comes as two keys.
    pub character: u16,
    pub down: bool,
    /// Which of Shift, Ctrl, Alt and the lock keys were on, as Windows
    /// numbers them.
    pub modifiers: u32,
    /// How many times the key repeated while held down.
    pub repeats: u16,
}

/// The modifier bits that only say a lock key is on.
const LOCK_KEYS: u32 = 0x20 | 0x40 | 0x80;

/// Windows' numbers for the Alt key and for the digits of the number pad.
const ALT_KEY: u16 = 18;
const NUMBER_PAD_DIGITS: std::ops::RangeInclusive<u16> = 96..=105;

impl Key {
    /// A key that carries nothing, coming up: no key number and no
    /// character. An app is given one only to wake it.
    pub const NOTHING_COMING_UP: Key = Key {
        key: 0,
        scan_code: 0,
        character: 0,
        down: false,
        modifiers: 0,
        repeats: 1,
    };

    /// Reads a key code that names all six numbers. Any other code is not
    /// taken for a key, so that it reaches the hidden terminal as it came.
    fn read(code: &[u8]) -> Option<Key> {
        let numbers = code.strip_prefix(b"\x1b[")?.strip_suffix(b"_")?;
        let mut fields = [0u32; 6];
        let mut given = numbers.split(|&byte| byte == b';');
        for field in &mut fields {
            let number = given.next()?;
            if number.is_empty() || !number.iter().all(u8::is_ascii_digit) {
                return None;
            }
            *field = std::str::from_utf8(number).ok()?.parse().ok()?;
        }
        if given.next().is_some() || fields[3] > 1 {
            return None;
        }
        Some(Key {
            key: fields[0].try_into().ok()?,
            scan_code: fields[1].try_into().ok()?,
            character: fields[2].try_into().ok()?,
            down: fields[3] == 1,
            modifiers: fields[4],
            repeats: fields[5].try_into().ok()?,
        })
    }

    fn write(&self, out: &mut Vec<u8>) {
        let _ = write!(
            out,
            "\x1b[{};{};{};{};{};{}_",
            self.key,
            self.scan_code,
            self.character,
            u8::from(self.down),
            self.modifiers,
            self.repeats
        );
    }

    /// True when this may be one character of a code that an older console
    /// host spells out: a plain English character going down once, with no
    /// key number. `first` asks for the character every code starts with.
    fn spells(&self, first: bool) -> bool {
        self.key == 0
            && self.scan_code == 0
            && self.down
            && self.modifiers & !LOCK_KEYS == 0
            && self.repeats == 1
            && self.character < 0x80
            && (!first || self.character == u16::from(ESCAPE))
    }

    /// True for the Alt key going down with nothing typed. That is how a
    /// character typed by its number starts: for a pasted character the
    /// keyboard has no key for, such as a Half-space, an older console host
    /// makes up keys. Alt goes down, the character's number is typed on the
    /// number pad, and the character comes as Alt is let go.
    fn is_alt_going_down(&self) -> bool {
        self.key == ALT_KEY && self.down && self.character == 0
    }

    fn is_number_pad_digit(&self) -> bool {
        NUMBER_PAD_DIGITS.contains(&self.key) && self.character == 0
    }

    /// The character this key carries if it is Alt being let go after a
    /// number was typed.
    fn typed_by_number(&self) -> Option<char> {
        if self.key == ALT_KEY && !self.down && self.character != 0 {
            char::from_u32(self.character.into())
        } else {
            None
        }
    }
}

impl Input {
    /// Writes this as the hidden terminal must be given it. With
    /// `characters_as_keys`, a character outside plain English that came
    /// with no key is written as a key that carries it, going down and
    /// coming up: what a keyboard layout sends for it.
    pub fn write_for_app(&self, characters_as_keys: bool, out: &mut Vec<u8>) {
        match self {
            Input::Key(key) => key.write(out),
            Input::Character(character) if characters_as_keys && !character.is_ascii() => {
                for &unit in character.encode_utf16(&mut [0; 2]).iter() {
                    for down in [true, false] {
                        let key = Key {
                            key: 0,
                            scan_code: 0,
                            character: unit,
                            down,
                            modifiers: 0,
                            repeats: 1,
                        };
                        key.write(out);
                    }
                }
            }
            Input::Character(character) => {
                out.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes())
            }
            Input::PasteStarts => out.extend_from_slice(b"\x1b[200~"),
            Input::PasteEnds => out.extend_from_slice(b"\x1b[201~"),
            Input::Focus { gained: true } => out.extend_from_slice(b"\x1b[I"),
            Input::Focus { gained: false } => out.extend_from_slice(b"\x1b[O"),
            Input::Mouse(as_it_came) | Input::Other(as_it_came) => {
                out.extend_from_slice(as_it_came)
            }
        }
    }
}

/// Reads what the Terminal sends, however it is cut into pieces.
#[derive(Default)]
pub(crate) struct TerminalInput {
    /// The start of a code or character whose end has not come yet.
    unfinished: Vec<u8>,
    /// A code that is being spelled out: the keys that have come so far.
    spelling: Vec<Key>,
    /// What may be a character typed by its number: Alt going down, and
    /// the number pad keys that have come since.
    by_number: Vec<Key>,
}

impl TerminalInput {
    pub fn new() -> TerminalInput {
        TerminalInput::default()
    }

    /// Takes the next bytes the Terminal sent and returns what they hold,
    /// in order. A code or character cut off at the end is kept back until
    /// the rest of it arrives.
    pub fn read(&mut self, bytes: &[u8]) -> Vec<Input> {
        let mut waiting = std::mem::take(&mut self.unfinished);
        waiting.extend_from_slice(bytes);
        let mut inputs = Vec::new();
        let mut rest = &waiting[..];
        while let Some(length) = whole_code(rest) {
            let (code, after) = rest.split_at(length);
            self.take(code, &mut inputs);
            rest = after;
        }
        self.unfinished = rest.to_vec();
        inputs
    }

    /// True while the start of a code is kept back because its end has not
    /// come.
    pub fn holding(&self) -> bool {
        !self.unfinished.is_empty() || !self.spelling.is_empty() || !self.by_number.is_empty()
    }

    /// Gives what was kept back, as it came, because the rest has taken too
    /// long. A Terminal that does not send keys in the Windows form sends
    /// the Escape key as the character every code starts with, and only
    /// time tells the two apart.
    pub fn release(&mut self) -> Vec<Input> {
        let mut inputs = Vec::new();
        self.give_up_spelling(&mut inputs);
        self.give_up_number(&mut inputs);
        if !self.unfinished.is_empty() {
            inputs.push(Input::Other(std::mem::take(&mut self.unfinished)));
        }
        inputs
    }

    /// Takes one whole code or character.
    fn take(&mut self, code: &[u8], inputs: &mut Vec<Input>) {
        let Some(key) = Key::read(code) else {
            self.give_up_spelling(inputs);
            self.give_up_number(inputs);
            inputs.push(named(code));
            return;
        };
        if key.spells(self.spelling.is_empty()) {
            self.give_up_number(inputs);
            self.spelling.push(key);
            self.read_spelling(inputs);
            return;
        }
        self.give_up_spelling(inputs);
        let typing_a_number = !self.by_number.is_empty();
        if key.is_alt_going_down() || (typing_a_number && key.is_number_pad_digit()) {
            self.by_number.push(key);
        } else if let (true, Some(character)) = (typing_a_number, key.typed_by_number()) {
            self.by_number.clear();
            inputs.push(Input::Character(character));
        } else {
            self.give_up_number(inputs);
            inputs.push(Input::Key(key));
        }
    }

    fn give_up_number(&mut self, inputs: &mut Vec<Input>) {
        inputs.extend(self.by_number.drain(..).map(Input::Key));
    }

    /// Looks at what the keys so far spell. A paste mark, a mouse report or
    /// a focus change is given as that. Anything else stays the keys it
    /// came as, which is how an app that reads keys expects an answer.
    fn read_spelling(&mut self, inputs: &mut Vec<Input>) {
        loop {
            let spelled: Vec<u8> = self
                .spelling
                .iter()
                .map(|key| key.character as u8)
                .collect();
            let Some(length) = whole_code(&spelled) else {
                return;
            };
            if length == spelled.len() {
                match named(&spelled) {
                    input @ (Input::PasteStarts
                    | Input::PasteEnds
                    | Input::Mouse(_)
                    | Input::Focus { .. }) => {
                        self.spelling.clear();
                        inputs.push(input);
                    }
                    _ => self.give_up_spelling(inputs),
                }
                return;
            }
            // The last key did not go on with the code before it. What it
            // ended were keys, and it may start a code of its own.
            inputs.extend(self.spelling.drain(..length).map(Input::Key));
            if !self.spelling[0].spells(true) {
                self.give_up_spelling(inputs);
                return;
            }
        }
    }

    fn give_up_spelling(&mut self, inputs: &mut Vec<Input>) {
        inputs.extend(self.spelling.drain(..).map(Input::Key));
    }
}

/// What a whole code or character is.
fn named(code: &[u8]) -> Input {
    match code {
        b"\x1b[200~" => Input::PasteStarts,
        b"\x1b[201~" => Input::PasteEnds,
        b"\x1b[I" => Input::Focus { gained: true },
        b"\x1b[O" => Input::Focus { gained: false },
        // The three forms a Terminal reports the mouse in: the one both
        // Target apps ask for, and two older ones.
        [ESCAPE, b'[', b'<', .., b'M' | b'm']
        | [ESCAPE, b'[', b'0'..=b'9', .., b'M']
        | [ESCAPE, b'[', b'M', _, ..] => Input::Mouse(code.to_vec()),
        [ESCAPE, ..] => Input::Other(code.to_vec()),
        _ => {
            let mut characters = std::str::from_utf8(code).unwrap_or("").chars();
            match (characters.next(), characters.next()) {
                (Some(character), None) => Input::Character(character),
                _ => Input::Other(code.to_vec()),
            }
        }
    }
}

/// The length of the code or character `bytes` starts with, or `None` when
/// its end is not there yet.
fn whole_code(bytes: &[u8]) -> Option<usize> {
    match *bytes {
        [] | [ESCAPE] => None,
        [ESCAPE, b'[', ..] => control_sequence(bytes),
        // A string: the form of the Terminal's longer answers.
        [ESCAPE, b']', ..] => string(bytes, true),
        [ESCAPE, b'P' | b'_' | b'^' | b'X', ..] => string(bytes, false),
        // `ESC O` and a letter: some keys as older Terminals send them.
        [ESCAPE, b'O'] => None,
        [ESCAPE, b'O', ESCAPE, ..] => Some(2),
        [ESCAPE, b'O', ..] => Some(3),
        // The Escape key, then another code.
        [ESCAPE, ESCAPE, ..] => Some(1),
        // Escape before a character is that character with Alt held down.
        [ESCAPE, ..] => Some(1 + character(&bytes[1..])?),
        _ => character(bytes),
    }
}

/// A control sequence: `ESC [`, numbers, in-between characters and one
/// last character.
fn control_sequence(bytes: &[u8]) -> Option<usize> {
    let is_number = |byte: &u8| (0x30..=0x3f).contains(byte);
    let is_between = |byte: &u8| (0x20..=0x2f).contains(byte);
    let mut at = 2;
    at += bytes[at..]
        .iter()
        .take_while(|byte| is_number(byte))
        .count();
    at += bytes[at..]
        .iter()
        .take_while(|byte| is_between(byte))
        .count();
    match *bytes.get(at)? {
        // The oldest form of mouse report: three more characters follow.
        b'M' if at == 2 => {
            let mut end = at + 1;
            for _ in 0..3 {
                if bytes.get(end) == Some(&ESCAPE) {
                    break;
                }
                end += character(&bytes[end..])?;
            }
            Some(end)
        }
        0x40..=0x7e => Some(at + 1),
        // Broken off by something that cannot be part of it.
        _ => Some(at),
    }
}

/// A string code, which runs until its terminator: `ESC \`, or the bell
/// where that is allowed.
fn string(bytes: &[u8], ended_by_bell: bool) -> Option<usize> {
    let mut at = 2;
    loop {
        match *bytes.get(at)? {
            BELL if ended_by_bell => return Some(at + 1),
            ESCAPE => {
                return match *bytes.get(at + 1)? {
                    b'\\' => Some(at + 2),
                    // Broken off by the start of another code.
                    _ => Some(at),
                };
            }
            _ => at += 1,
        }
    }
}

/// The length in bytes of the character `bytes` starts with, or `None` when
/// some of its bytes are still to come.
fn character(bytes: &[u8]) -> Option<usize> {
    let length = match *bytes.first()? {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => return Some(1),
    };
    let following = bytes[1..]
        .iter()
        .take(length - 1)
        .take_while(|byte| (0x80..0xc0).contains(*byte))
        .count();
    if following == length - 1 {
        Some(length)
    } else if 1 + following == bytes.len() {
        None
    } else {
        // Not a character at all: a byte that stands alone.
        Some(1)
    }
}

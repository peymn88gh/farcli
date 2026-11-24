//! Splits what a Target app wrote into the codes that draw on its screen and
//! the codes that ask the Terminal itself for something.
//!
//! Drawing codes go to the Hidden screen, and the Terminal sees only what the
//! Painter makes of them. Everything else (window title, keyboard and mouse
//! modes, questions to the Terminal, the bell) means the same to the Terminal
//! whether farcli is there or not, so it is passed on unchanged.

use crate::screen::EraseScreen;

/// One piece of a Target app's output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Piece {
    /// Text and drawing codes.
    Drawing(Vec<u8>),
    /// The drawing code that clears part or all of the screen.
    Erase(EraseScreen, Vec<u8>),
    /// The drawing code that moves between the main screen and the
    /// alternate screen that full-screen programs draw on.
    ScreenSwitch(Vec<u8>),
    /// A code for the Terminal itself.
    ForTerminal(Vec<u8>),
    /// The app asks where its cursor is.
    CursorPositionAsked,
    /// Someone asks what kind of terminal this is: the app, or the hidden
    /// terminal itself when it starts.
    TerminalKindAsked(Vec<u8>),
    /// The app starts drawing a frame that must not be shown half done.
    FrameStarts,
    /// The frame is complete.
    FrameEnds,
}

/// Where the splitter is inside the app's output. The app may stop writing
/// anywhere, even in the middle of a code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Text,
    /// After the escape character.
    Escape,
    /// In a short escape code, after at least one in-between character.
    EscapeMore,
    /// In a control sequence (`ESC [`).
    Control,
    /// In a string code, which runs until its terminator.
    String {
        ended_by_bell: bool,
    },
    /// In a string code, after an escape character that may start the
    /// terminator.
    StringEscape {
        ended_by_bell: bool,
    },
}

pub(crate) struct Splitter {
    state: State,
    /// The code being read, from its escape character on.
    code: Vec<u8>,
    pieces: Vec<Piece>,
}

const ESCAPE: u8 = 0x1b;
const BELL: u8 = 0x07;

impl Splitter {
    pub fn new() -> Splitter {
        Splitter {
            state: State::Text,
            code: Vec::new(),
            pieces: Vec::new(),
        }
    }

    /// Splits the next bytes the app wrote. A code cut off at the end is
    /// kept back until the rest of it arrives.
    pub fn split(&mut self, bytes: &[u8]) -> Vec<Piece> {
        for &byte in bytes {
            self.take(byte);
        }
        std::mem::take(&mut self.pieces)
    }

    fn take(&mut self, byte: u8) {
        match self.state {
            State::Text => match byte {
                ESCAPE => self.start_code(),
                BELL => self.pieces.push(Piece::ForTerminal(vec![BELL])),
                _ => self.draw(&[byte]),
            },
            State::Escape => {
                self.code.push(byte);
                match byte {
                    ESCAPE => self.start_code(),
                    b'[' => self.state = State::Control,
                    b']' => {
                        self.state = State::String {
                            ended_by_bell: true,
                        }
                    }
                    b'P' | b'_' | b'^' | b'X' => {
                        self.state = State::String {
                            ended_by_bell: false,
                        }
                    }
                    0x20..=0x2f => self.state = State::EscapeMore,
                    0x30..=0x7e => self.finish_code(short_escape_piece),
                    _ => self.finish_code(drawing_piece),
                }
            }
            State::EscapeMore => {
                self.code.push(byte);
                match byte {
                    ESCAPE => self.start_code(),
                    0x20..=0x2f => {}
                    0x30..=0x7e => self.finish_code(short_escape_piece),
                    _ => self.finish_code(drawing_piece),
                }
            }
            State::Control => {
                self.code.push(byte);
                match byte {
                    ESCAPE => self.start_code(),
                    0x40..=0x7e => self.finish_code(control_piece),
                    _ => {}
                }
            }
            State::String { ended_by_bell } => {
                self.code.push(byte);
                match byte {
                    BELL if ended_by_bell => self.finish_code(string_piece),
                    ESCAPE => self.state = State::StringEscape { ended_by_bell },
                    // Cancel and substitute end a string without running it.
                    0x18 | 0x1a => self.finish_code(drawing_piece),
                    _ => {}
                }
            }
            State::StringEscape { .. } => {
                if byte == b'\\' {
                    self.code.push(byte);
                    self.finish_code(string_piece);
                } else {
                    // The escape character ended the string and starts a
                    // new code. Give the string its proper terminator.
                    self.code.push(b'\\');
                    self.finish_code(string_piece);
                    self.start_code();
                    self.take(byte);
                }
            }
        }
    }

    fn start_code(&mut self) {
        self.code.clear();
        self.code.push(ESCAPE);
        self.state = State::Escape;
    }

    fn finish_code(&mut self, piece_for: fn(Vec<u8>) -> Vec<Piece>) {
        self.state = State::Text;
        for piece in piece_for(std::mem::take(&mut self.code)) {
            match piece {
                Piece::Drawing(bytes) => self.draw(&bytes),
                other => self.pieces.push(other),
            }
        }
    }

    fn draw(&mut self, bytes: &[u8]) {
        if let Some(Piece::Drawing(drawing)) = self.pieces.last_mut() {
            drawing.extend_from_slice(bytes);
        } else {
            self.pieces.push(Piece::Drawing(bytes.to_vec()));
        }
    }
}

fn drawing_piece(code: Vec<u8>) -> Vec<Piece> {
    vec![Piece::Drawing(code)]
}

/// A code of the escape character, optional in-between characters and one
/// final character.
fn short_escape_piece(code: Vec<u8>) -> Vec<Piece> {
    let drawing = match &code[1..] {
        // Save and restore cursor, index, next line, tab set, reverse index,
        // full reset, and the character set shifts.
        [b'7' | b'8' | b'D' | b'E' | b'H' | b'M' | b'c' | b'n' | b'o' | b'|' | b'}' | b'~'] => true,
        // A string terminator with no string before it.
        [b'\\'] => return Vec::new(),
        // Choosing character sets, and the screen alignment test.
        [b'(' | b')' | b'*' | b'+' | b'-' | b'.' | b'/' | b'#' | b'%', ..] => true,
        _ => false,
    };
    if drawing {
        vec![Piece::Drawing(code)]
    } else {
        vec![Piece::ForTerminal(code)]
    }
}

/// An operating system command, device control string or other string code.
fn string_piece(code: Vec<u8>) -> Vec<Piece> {
    // A link is part of how cells are drawn. Every other string is for the
    // Terminal.
    if code.starts_with(b"\x1b]8;") {
        vec![Piece::Drawing(code)]
    } else {
        vec![Piece::ForTerminal(code)]
    }
}

/// A control sequence: `ESC [`, parameters, in-between characters and one
/// final character.
fn control_piece(code: Vec<u8>) -> Vec<Piece> {
    let body = &code[2..code.len() - 1];
    let last = code[code.len() - 1];
    let (marker, body) = match body.first() {
        Some(&marker @ b'<'..=b'?') => (Some(marker), &body[1..]),
        _ => (None, body),
    };
    let params_len = body
        .iter()
        .position(|byte| !matches!(byte, b'0'..=b'9' | b';' | b':'))
        .unwrap_or(body.len());
    let (params, between) = body.split_at(params_len);
    if between.iter().any(|byte| !matches!(byte, 0x20..=0x2f)) {
        // Not a well-formed code. The emulator decides what it means.
        return vec![Piece::Drawing(code)];
    }
    let first_param = || {
        params
            .split(|&byte| byte == b';')
            .next()
            .and_then(|param| std::str::from_utf8(param).ok())
            .and_then(|param| param.parse::<u16>().ok())
            .unwrap_or(0)
    };

    match (marker, between, last) {
        (None, [], b'J') => match first_param() {
            0 => vec![Piece::Erase(EraseScreen::Below, code)],
            1 => vec![Piece::Erase(EraseScreen::Above, code)],
            2 => vec![Piece::Erase(EraseScreen::All, code)],
            3 => vec![Piece::Erase(EraseScreen::Scrollback, code)],
            _ => vec![Piece::Drawing(code)],
        },
        (None, [], b'n') if first_param() == 6 => vec![Piece::CursorPositionAsked],
        (None, [], b'c') if first_param() == 0 => vec![Piece::TerminalKindAsked(code)],
        // The hidden terminal says that its own window, which nobody sees,
        // was shown or hidden. That is not the Terminal's window.
        (None, [], b't') if matches!(params, b"1" | b"2") => Vec::new(),
        // Moving the cursor, erasing and inserting, scrolling, tab stops,
        // styles, margins, saving the cursor and the non-private modes.
        (
            None,
            [],
            b'@'
            | b'A'..=b'I'
            | b'K'..=b'M'
            | b'P'
            | b'S'
            | b'T'
            | b'X'
            | b'Z'
            | b'`'
            | b'a'
            | b'b'
            | b'd'..=b'h'
            | b'l'
            | b'm'
            | b'r'
            | b's'
            | b'u',
        ) => vec![Piece::Drawing(code)],
        (Some(b'?'), [], b'h' | b'l') => private_mode_pieces(params, last),
        // Erasing only the cells that are not protected.
        (Some(b'?'), [], b'J' | b'K') => vec![Piece::Drawing(code)],
        // Soft reset, and protecting cells from erasure.
        (None, [b'!'], b'p') | (None, [b'"'], b'q') => vec![Piece::Drawing(code)],
        // Filling, erasing and copying rectangles are drawing. Kept from the
        // Terminal even though the emulator does not know them, so that the
        // Terminal never holds something the Hidden screen does not.
        (None, [b'$'], last) if last != b'p' => vec![Piece::Drawing(code)],
        _ => vec![Piece::ForTerminal(code)],
    }
}

/// Splits a private mode code into one code per mode, each sent where that
/// mode belongs.
fn private_mode_pieces(params: &[u8], last: u8) -> Vec<Piece> {
    params
        .split(|&byte| byte == b';')
        .filter_map(|param| {
            let code = [b"\x1b[?", param, &[last]].concat();
            let mode = std::str::from_utf8(param)
                .ok()
                .and_then(|param| param.parse::<u16>().ok());
            Some(match mode {
                // Wide screen, cursor origin, wrapping at the right edge,
                // cursor visibility, wrapping backwards, side margins, and
                // saving the cursor.
                Some(3 | 6 | 7 | 25 | 45 | 69 | 1048) => Piece::Drawing(code),
                Some(47 | 1047 | 1049) => Piece::ScreenSwitch(code),
                Some(2026) if last == b'h' => Piece::FrameStarts,
                Some(2026) => Piece::FrameEnds,
                // The hidden terminal asks for keys in the form Windows
                // keeps them in. That is the form farcli always sends, and
                // nothing the Terminal should be asked for.
                Some(9001) => return None,
                _ => Piece::ForTerminal(code),
            })
        })
        .collect()
}

//! Recordings: a farcli session written to a file, and played again through
//! the session with no real app, no real Terminal and no clock.
//!
//! A recording holds what the session was fed: the bytes the Target app
//! wrote, the keys the user pressed and the Terminal's size, in the order
//! they came. It does not hold what farcli sent the Terminal: playing the
//! recording works that out again.
//!
//! The file is text, one line per thing that happened:
//!
//! ```text
//! farcli recording 1
//! app: claude
//! app-version: 2.1.322 (Claude Code)
//! system: Windows native
//! size: 100x30
//! cursor: 3,0
//!
//! 0 app \e[?25l\e[4;1Hhello
//! 950 keys \e[65;30;97;1;0;1_
//! 2000 size 120x30
//! 2150 release
//! ```
//!
//! The number is milliseconds since the session started. `app` is output
//! from the Target app, `keys` is what the user typed as it reached farcli,
//! `size` is a resize of the Terminal, and `release` is the moment farcli
//! stopped waiting for the app to finish a frame. Bytes are written as
//! text where they are text; see `escaped` for the rest.

use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::Path;

use crate::app_profile::AppProfile;
use crate::fake_terminal::FakeTerminal;
use crate::screen::Position;
use crate::session::{Reaction, Session};
use crate::Size;

const FIRST_LINE: &str = "farcli recording 1";

/// A recorded session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    pub about: About,
    /// Everything that happened, each with the milliseconds since the
    /// session started.
    pub events: Vec<(u64, Event)>,
}

/// What a recording says about the session it was made from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct About {
    /// The Target app as the user named it after `farcli`.
    pub app: String,
    pub app_version: String,
    /// The system the session ran on, such as `Windows native` or `WSL`.
    pub system: String,
    /// The Terminal's size when the session started.
    pub size: Size,
    /// Where the Terminal's cursor was when the session started.
    pub cursor: Position,
    /// Anything else worth knowing, as name and value: the display mode,
    /// the command line, which hidden terminal produced the bytes.
    pub notes: Vec<(String, String)>,
}

/// One thing that happened in a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The Target app wrote these bytes.
    AppWrote(Vec<u8>),
    /// The user pressed keys, which reached farcli as these bytes.
    UserTyped(Vec<u8>),
    /// The user gave the Terminal's window a new size.
    TerminalResized(Size),
    /// The Target app took too long to finish a frame, and farcli showed
    /// what it had.
    HeldFrameReleased,
}

/// Writes a recording as the session goes, one line per event, so that a
/// session that ends badly still leaves everything up to that point.
pub struct Recorder<W: Write> {
    file: W,
}

impl<W: Write> Recorder<W> {
    pub fn start(mut file: W, about: &About) -> io::Result<Recorder<W>> {
        let mut head = format!("{FIRST_LINE}\n");
        let _ = writeln!(head, "app: {}", one_line(&about.app));
        let _ = writeln!(head, "app-version: {}", one_line(&about.app_version));
        let _ = writeln!(head, "system: {}", one_line(&about.system));
        let _ = writeln!(head, "size: {}x{}", about.size.cols, about.size.rows);
        let _ = writeln!(head, "cursor: {},{}", about.cursor.row, about.cursor.col);
        for (name, value) in &about.notes {
            let _ = writeln!(head, "{}: {}", one_line(name), one_line(value));
        }
        head.push('\n');
        file.write_all(head.as_bytes())?;
        file.flush()?;
        Ok(Recorder { file })
    }

    /// Adds something that happened `milliseconds` after the session started.
    pub fn record(&mut self, milliseconds: u64, event: &Event) -> io::Result<()> {
        let line = match event {
            Event::AppWrote(bytes) => format!("{milliseconds} app {}\n", escaped(bytes)),
            Event::UserTyped(bytes) => format!("{milliseconds} keys {}\n", escaped(bytes)),
            Event::TerminalResized(size) => {
                format!("{milliseconds} size {}x{}\n", size.cols, size.rows)
            }
            Event::HeldFrameReleased => format!("{milliseconds} release\n"),
        };
        self.file.write_all(line.as_bytes())?;
        self.file.flush()
    }
}

impl Recording {
    /// Reads a recording from the text of its file.
    pub fn read(text: &str) -> Result<Recording, String> {
        let mut lines = text.lines().enumerate().map(|(index, line)| {
            // Git may hand the file out with Windows line ends.
            (index + 1, line.strip_suffix('\r').unwrap_or(line))
        });
        match lines.next() {
            Some((_, FIRST_LINE)) => {}
            _ => return Err(format!("the first line is not `{FIRST_LINE}`")),
        }

        let mut app = None;
        let mut app_version = None;
        let mut system = None;
        let mut size = None;
        let mut cursor = None;
        let mut notes = Vec::new();
        for (number, line) in lines.by_ref() {
            if line.is_empty() {
                break;
            }
            let (name, value) = line
                .split_once(": ")
                .or_else(|| line.strip_suffix(':').map(|name| (name, "")))
                .ok_or_else(|| format!("line {number}: expected `name: value`"))?;
            match name {
                "app" => app = Some(value.to_string()),
                "app-version" => app_version = Some(value.to_string()),
                "system" => system = Some(value.to_string()),
                "size" => {
                    size =
                        Some(parse_size(value).ok_or_else(|| format!("line {number}: bad size"))?)
                }
                "cursor" => {
                    cursor = Some(
                        parse_cursor(value).ok_or_else(|| format!("line {number}: bad cursor"))?,
                    )
                }
                _ => notes.push((name.to_string(), value.to_string())),
            }
        }
        let about = About {
            app: app.ok_or("the recording does not name its app")?,
            app_version: app_version.ok_or("the recording does not give the app's version")?,
            system: system.ok_or("the recording does not give its system")?,
            size: size.ok_or("the recording does not give the Terminal's size")?,
            cursor: cursor.ok_or("the recording does not give the Terminal's cursor")?,
            notes,
        };

        let mut events = Vec::new();
        for (number, line) in lines {
            if line.is_empty() {
                continue;
            }
            let bad = |what: &str| format!("line {number}: {what}");
            let (milliseconds, rest) = line.split_once(' ').ok_or_else(|| bad("no event"))?;
            let milliseconds = milliseconds.parse().map_err(|_| bad("bad time"))?;
            let (kind, value) = rest.split_once(' ').unwrap_or((rest, ""));
            let event = match kind {
                "app" => Event::AppWrote(unescaped(value).map_err(|what| bad(&what))?),
                "keys" => Event::UserTyped(unescaped(value).map_err(|what| bad(&what))?),
                "size" => Event::TerminalResized(parse_size(value).ok_or_else(|| bad("bad size"))?),
                "release" => Event::HeldFrameReleased,
                _ => return Err(bad("unknown kind of event")),
            };
            events.push((milliseconds, event));
        }
        Ok(Recording { about, events })
    }

    /// Reads a recording file.
    pub fn load(file: impl AsRef<Path>) -> io::Result<Recording> {
        let file = file.as_ref();
        let text = std::fs::read_to_string(file)?;
        Recording::read(&text).map_err(|what| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {what}", file.display()),
            )
        })
    }

    /// The value of the note called `name`, if the recording has one.
    pub fn note(&self, name: &str) -> Option<&str> {
        self.about
            .notes
            .iter()
            .find(|(noted, _)| noted == name)
            .map(|(_, value)| value.as_str())
    }
}

/// A recording being played through a session, joined to the Terminal that
/// shows the session's output.
pub struct Replay {
    session: Session,
    /// What a Terminal shows after everything played so far.
    pub terminal: FakeTerminal,
    /// Everything forwarded to the Target app so far.
    pub app_received: Vec<u8>,
    /// Everything sent to the Terminal so far.
    pub terminal_received: Vec<u8>,
    events: std::vec::IntoIter<(u64, Event)>,
}

impl Replay {
    /// Starts playing `recording` with nothing played yet, in a Terminal of
    /// the recorded size with its cursor where it was.
    pub fn start(recording: &Recording, profile: AppProfile) -> Replay {
        let about = &recording.about;
        let mut terminal = FakeTerminal::new(about.size);
        terminal
            .receive(format!("\x1b[{};{}H", about.cursor.row + 1, about.cursor.col + 1).as_bytes());
        Replay {
            session: Session::start(profile, about.size, about.cursor),
            terminal,
            app_received: Vec::new(),
            terminal_received: Vec::new(),
            events: recording.events.clone().into_iter(),
        }
    }

    /// Plays the next thing that happened and returns it, or `None` when
    /// the recording is over.
    pub fn step(&mut self) -> Option<Event> {
        let (_, event) = self.events.next()?;
        let reaction = match &event {
            Event::AppWrote(bytes) => self.session.app_wrote(bytes),
            Event::UserTyped(bytes) => self.session.user_typed(bytes),
            Event::TerminalResized(size) => {
                self.terminal.resize(*size);
                self.session.terminal_resized(*size)
            }
            Event::HeldFrameReleased => self.session.release_held_frame(),
        };
        self.take(reaction);
        Some(event)
    }

    /// Plays everything that is left.
    pub fn finish(&mut self) {
        while self.step().is_some() {}
        if self.session.holding_frame() {
            let reaction = self.session.release_held_frame();
            self.take(reaction);
        }
    }

    /// Plays on until `reached` says so, checking after each thing that
    /// happened. Returns false when the recording ended first.
    pub fn play_until(&mut self, mut reached: impl FnMut(&Replay) -> bool) -> bool {
        while !reached(self) {
            if self.step().is_none() {
                return false;
            }
        }
        true
    }

    fn take(&mut self, reaction: Reaction) {
        self.terminal.receive(&reaction.for_terminal);
        self.terminal_received.extend(reaction.for_terminal);
        self.app_received.extend(reaction.for_app);
    }
}

fn parse_size(text: &str) -> Option<Size> {
    let (cols, rows) = text.split_once('x')?;
    Some(Size {
        cols: cols.parse().ok()?,
        rows: rows.parse().ok()?,
    })
}

fn parse_cursor(text: &str) -> Option<Position> {
    let (row, col) = text.split_once(',')?;
    Some(Position {
        row: row.parse().ok()?,
        col: col.parse().ok()?,
    })
}

/// A header value kept to its one line.
fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ").trim().to_string()
}

/// Bytes as one line of text that a person can read and search.
///
/// Text stays text. `\e` is the escape byte, `\r` `\n` `\t` the usual ones
/// and `\\` a backslash. A character nobody can see (a Half-space, a
/// direction mark) is written `\u{200c}`. Any other byte, and any byte that
/// is not part of a whole character, is written `\xNN`.
fn escaped(bytes: &[u8]) -> String {
    let mut text = String::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let (valid, bad) = match std::str::from_utf8(rest) {
            Ok(valid) => (valid, &[][..]),
            Err(error) => {
                let (valid, after) = rest.split_at(error.valid_up_to());
                let bad_len = error.error_len().unwrap_or(after.len());
                // The bytes before the error were just checked.
                (std::str::from_utf8(valid).unwrap(), &after[..bad_len])
            }
        };
        for character in valid.chars() {
            match character {
                '\x1b' => text.push_str("\\e"),
                '\r' => text.push_str("\\r"),
                '\n' => text.push_str("\\n"),
                '\t' => text.push_str("\\t"),
                '\\' => text.push_str("\\\\"),
                '\0'..='\x1f' | '\x7f' => {
                    let _ = write!(text, "\\x{:02x}", character as u32);
                }
                unseen if is_unseen(unseen) => {
                    let _ = write!(text, "\\u{{{:04x}}}", unseen as u32);
                }
                other => text.push(other),
            }
        }
        for byte in bad {
            let _ = write!(text, "\\x{byte:02x}");
        }
        rest = &rest[valid.len() + bad.len()..];
    }
    // An editor may drop a space at the end of a line.
    if text.ends_with(' ') {
        text.pop();
        text.push_str("\\x20");
    }
    text
}

/// True for a character that shows nothing by itself, or that some programs
/// treat as the end of a line.
fn is_unseen(character: char) -> bool {
    matches!(
        character,
        '\u{80}'..='\u{9f}'
            | '\u{ad}'
            | '\u{61c}'
            | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{feff}'
    )
}

fn unescaped(text: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            bytes.extend(character.encode_utf8(&mut [0; 4]).as_bytes());
            continue;
        }
        match characters.next() {
            Some('e') => bytes.push(0x1b),
            Some('r') => bytes.push(b'\r'),
            Some('n') => bytes.push(b'\n'),
            Some('t') => bytes.push(b'\t'),
            Some('\\') => bytes.push(b'\\'),
            Some('x') => {
                let digits: String = characters.by_ref().take(2).collect();
                let byte = u8::from_str_radix(&digits, 16)
                    .ok()
                    .filter(|_| digits.len() == 2)
                    .ok_or_else(|| format!("bad byte `\\x{digits}`"))?;
                bytes.push(byte);
            }
            Some('u') => {
                let digits: String = characters
                    .by_ref()
                    .skip(1)
                    .take_while(|digit| *digit != '}')
                    .collect();
                let character = u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| format!("bad character `\\u{{{digits}}}`"))?;
                bytes.extend(character.encode_utf8(&mut [0; 4]).as_bytes());
            }
            other => return Err(format!("unknown code `\\{}`", other.unwrap_or(' '))),
        }
    }
    Ok(bytes)
}

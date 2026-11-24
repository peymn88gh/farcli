//! A stand-in for Claude Code with a message in its Input box: it takes the
//! keys the user presses, keeps the message as the app does, and draws the
//! box as Claude Code 2.1.322 was seen to draw it on Windows.
//!
//! What it copies from the real app, each seen in the recordings of typing
//! or in a live session in which nothing was sent:
//!
//! - The box is a border, the rows of the message and a border. The first
//!   row starts with the prompt mark and a no-break space, and the message
//!   is drawn from the third column.
//! - The app wraps the message four columns short of the Terminal's width,
//!   at a space, and keeps that space on the end of the row. A word longer
//!   than a row is cut where the row is full, with no place for the cursor
//!   between its two parts. A line break starts a new row.
//! - Left and Right move the cursor by one character of the message in
//!   Logical order, the space the app wrapped at and a line break each
//!   counting as one. With Shift held they do the same. With Ctrl or Alt
//!   held they go to the start of the word before the cursor and to the end
//!   of the word after it. At either end of the message they do nothing.
//! - Each row is put into Screen order on its own, by its own first letter
//!   (`as_drawn`).
//! - The spaces at the end of a row are not drawn, except up to the app's
//!   cursor: with the cursor at the end of the message there is one empty
//!   cell, which a row drawn right to left shows on its left.
//! - The Terminal's cursor is in the column of the third plus the cells
//!   before it on its row in Logical order.
//! - A Half-space or a vowel mark stays with the letter before it: the
//!   cursor steps over both as one, and Backspace deletes both.
//! - With no message, the box shows a hint of the app's own, drawn dim.

use super::{as_drawn, at, direction_of, Run};
use farcli::layout::Direction;
use farcli::screen::Position;

/// A key of the keyboard, or a paste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Character(char),
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Backspace,
    Delete,
    Enter,
    Escape,
    /// Any other key or report, here the F1 key: the stand-in does nothing
    /// for it.
    Other,
    /// Left with Ctrl held: to the start of the word before the cursor.
    CtrlLeft,
    /// Right with Ctrl held: to the end of the word after the cursor.
    CtrlRight,
    /// Left and Right with Shift held, which the app takes for the plain
    /// arrows.
    ShiftLeft,
    ShiftRight,
    /// Ctrl and J together: a line break.
    LineBreak,
    /// Ctrl and W together: deletes the word before the cursor.
    DeleteWord,
    /// Ctrl and K together: deletes from the cursor to the end of the line.
    DeleteToEnd,
    /// Text pasted from elsewhere, between the marks of a paste.
    Paste(String),
}

impl Key {
    /// What Windows Terminal sends farcli for the key: a code for the key
    /// going down and one for it coming up, with Windows' number for the
    /// key where the key has one.
    pub fn sent_by_the_terminal(&self) -> String {
        let key = |number: u32, scan: u32, character: u32, held: u32| {
            format!(
                "\x1b[{number};{scan};{character};1;{held};1_\x1b[{number};{scan};{character};0;{held};1_"
            )
        };
        // A key pressed with Ctrl held: Ctrl goes down before it and comes
        // up after it.
        let with_ctrl = |keys: String| format!("\x1b[17;29;0;1;8;1_{keys}\x1b[17;29;0;0;0;1_");
        let with_shift = |keys: String| format!("[16;42;0;1;16;1_{keys}[16;42;0;0;0;1_");
        // The keys beside the letters come marked as such.
        const BESIDE: u32 = 256;
        match self {
            Key::Character(character) => {
                // The letters and digits of an English keyboard have keys of
                // their own. A Farsi letter comes with no key number.
                let number = match character {
                    'a'..='z' | 'A'..='Z' => u32::from(character.to_ascii_uppercase()),
                    '0'..='9' | ' ' => u32::from(*character),
                    _ => 0,
                };
                key(number, 0, u32::from(*character), 0)
            }
            Key::Left => key(37, 75, 0, BESIDE),
            Key::Right => key(39, 77, 0, BESIDE),
            Key::Up => key(38, 72, 0, BESIDE),
            Key::Down => key(40, 80, 0, BESIDE),
            Key::Home => key(36, 71, 0, BESIDE),
            Key::End => key(35, 79, 0, BESIDE),
            Key::Delete => key(46, 83, 0, BESIDE),
            Key::Backspace => key(8, 14, 8, 0),
            Key::Enter => key(13, 28, 13, 0),
            Key::Escape => key(27, 1, 27, 0),
            Key::Other => key(112, 59, 0, 0),
            Key::CtrlLeft => with_ctrl(key(37, 75, 0, BESIDE | 8)),
            Key::CtrlRight => with_ctrl(key(39, 77, 0, BESIDE | 8)),
            Key::ShiftLeft => with_shift(key(37, 75, 0, BESIDE | 16)),
            Key::ShiftRight => with_shift(key(39, 77, 0, BESIDE | 16)),
            Key::LineBreak => with_ctrl(key(74, 36, 10, 8)),
            Key::DeleteWord => with_ctrl(key(87, 17, 23, 8)),
            Key::DeleteToEnd => with_ctrl(key(75, 37, 11, 8)),
            Key::Paste(text) => format!("\x1b[200~{text}\x1b[201~"),
        }
    }

    /// The keys the app reads from what farcli forwarded to it: each key
    /// going down, in order. A key coming up does nothing in the app.
    fn read_by_the_app(forwarded: &str) -> Vec<Key> {
        let mut keys = Vec::new();
        let mut rest = forwarded;
        while let Some(character) = rest.chars().next() {
            if let Some(pasted) = rest.strip_prefix("\x1b[200~") {
                let (text, after) = pasted.split_once("\x1b[201~").expect("a paste with no end");
                keys.push(Key::Paste(text.to_string()));
                rest = after;
            } else if let Some(code) = rest.strip_prefix("\x1b[") {
                let end = code
                    .find(|last: char| ('\x40'..='\x7e').contains(&last))
                    .expect("a code with no end");
                let (numbers, last) = code.split_at(end);
                keys.extend(Key::named_by_code(numbers, &last[..1]));
                rest = &last[1..];
            } else {
                keys.push(match character {
                    '\u{8}' | '\u{7f}' => Key::Backspace,
                    '\n' => Key::LineBreak,
                    character => Key::Character(character),
                });
                rest = &rest[character.len_utf8()..];
            }
        }
        keys
    }

    /// The keys a code names: `ESC [`, then `numbers`, then `last`.
    fn named_by_code(numbers: &str, last: &str) -> Vec<Key> {
        if last != "_" {
            // A key as Terminals send it that do not send keys in the
            // Windows form.
            return match (numbers, last) {
                ("", "D") => vec![Key::Left],
                ("", "C") => vec![Key::Right],
                ("1;5", "D") => vec![Key::CtrlLeft],
                ("1;5", "C") => vec![Key::CtrlRight],
                ("3", "~") => vec![Key::Delete],
                _ => vec![Key::Other],
            };
        }
        let fields: Vec<u32> = numbers.split(';').map(|n| n.parse().unwrap()).collect();
        let [number, _, character, down, held, repeats] = fields[..] else {
            panic!("a key code names six numbers: {numbers:?}");
        };
        const ALT: u32 = 0x1 | 0x2;
        const CTRL: u32 = 0x4 | 0x8;
        let key = match number {
            _ if down == 0 => return Vec::new(),
            // Shift, Ctrl and Alt do nothing by themselves.
            16..=18 => return Vec::new(),
            37 if held & (CTRL | ALT) != 0 => Key::CtrlLeft,
            39 if held & (CTRL | ALT) != 0 => Key::CtrlRight,
            74 if held & CTRL != 0 => Key::LineBreak,
            87 if held & CTRL != 0 => Key::DeleteWord,
            75 if held & CTRL != 0 => Key::DeleteToEnd,
            37 => Key::Left,
            39 => Key::Right,
            38 => Key::Up,
            40 => Key::Down,
            36 => Key::Home,
            35 => Key::End,
            46 => Key::Delete,
            8 => Key::Backspace,
            13 => Key::Enter,
            27 => Key::Escape,
            // A key that types a character: not one pressed with Ctrl or
            // Alt alone, which is a shortcut of the app's.
            _ if character >= 32 && (held & CTRL != 0) == (held & ALT != 0) => {
                Key::Character(char::from_u32(character).unwrap())
            }
            _ => Key::Other,
        };
        vec![key; repeats as usize]
    }
}

/// Claude Code with its Input box on screen, run through farcli.
pub struct ClaudeCode {
    pub run: Run,
    cols: usize,
    /// The message: each character with what rides on it, and `"\n"` for a
    /// line break.
    message: Vec<String>,
    /// The place in the message before which the next character goes.
    cursor: usize,
    /// What the app has not handled yet: for each key the user pressed,
    /// the keys farcli forwarded for it.
    not_handled: Vec<Vec<Key>>,
    /// Everything farcli forwarded to the app.
    pub sent_to_app: String,
    /// What the box's first row starts with.
    pub mark: String,
    /// Whether the app shows the Terminal's cursor.
    pub shows_cursor: bool,
    /// The text of a hint the app shows in an empty box.
    pub hint: String,
}

/// The row of the screen the box's upper border is on.
const TOP: usize = 2;

fn takes_no_cell(character: char) -> bool {
    matches!(character, '\u{200c}' | '\u{200d}')
        || unicode_bidi::bidi_class(character) == unicode_bidi::BidiClass::NSM
}

impl ClaudeCode {
    /// Starts the app in a Terminal `cols` wide, with an empty Input box.
    pub fn start(cols: u16, rows: u16) -> ClaudeCode {
        let mut app = ClaudeCode {
            run: Run::start(cols, rows),
            cols: usize::from(cols),
            message: Vec::new(),
            cursor: 0,
            not_handled: Vec::new(),
            sent_to_app: String::new(),
            mark: "❯\u{a0}".to_string(),
            shows_cursor: true,
            hint: "Try \"fix lint errors\"".to_string(),
        };
        app.run.app_writes("\x1b[2J\x1b[H Claude Code\r\n");
        app.draws();
        app
    }

    /// The user presses `key`. farcli forwards something for it, which the
    /// app has not handled yet. Returns what farcli forwarded.
    pub fn press(&mut self, key: Key) -> String {
        self.press_as(&key.sent_by_the_terminal())
    }

    /// The user presses a key in a Terminal that sends `sent` for it.
    /// Returns what farcli forwarded to the app.
    pub fn press_as(&mut self, sent: &str) -> String {
        let reaction = self.run.session.user_typed(sent.as_bytes());
        self.run.terminal.receive(&reaction.for_terminal);
        let forwarded = String::from_utf8(reaction.for_app).unwrap();
        self.sent_to_app.push_str(&forwarded);
        self.not_handled.push(Key::read_by_the_app(&forwarded));
        forwarded
    }

    /// The keys pressed so far and not handled went to something else the
    /// app was showing: they do nothing to the message.
    pub fn takes_the_keys_for_something_else(&mut self) {
        self.not_handled.clear();
    }

    /// The user gives the Terminal's window a new width, and the app draws
    /// its box again in it.
    pub fn is_resized(&mut self, cols: u16, rows: u16) {
        let size = farcli::Size { cols, rows };
        self.cols = usize::from(cols);
        self.run.terminal.resize(size);
        self.run.without_farcli.resize(size);
        let reaction = self.run.session.terminal_resized(size);
        self.run.terminal.receive(&reaction.for_terminal);
        self.run.app_writes("\x1b[2J\x1b[H Claude Code\r\n");
        self.draws();
    }

    /// The user presses the key of each character of `text`, and the app
    /// has handled none of them yet.
    pub fn press_keys(&mut self, text: &str) {
        for character in text.chars() {
            self.press(Key::Character(character));
        }
    }

    /// The app handles what farcli forwarded for the first `keys` of the
    /// keys the user pressed that it has not handled, and draws.
    pub fn handles(&mut self, keys: usize) {
        let forwarded: Vec<Key> = self.not_handled.drain(..keys).flatten().collect();
        for key in forwarded {
            self.handle(key);
        }
        self.draws();
    }

    /// The user presses `key`, and the app handles what farcli forwarded
    /// for it and draws. Returns what farcli forwarded.
    pub fn presses(&mut self, key: Key) -> String {
        let forwarded = self.press(key);
        self.handles(self.not_handled.len());
        forwarded
    }

    /// The user types `text` one key at a time, and the app draws after
    /// each key.
    pub fn types(&mut self, text: &str) {
        for character in text.chars() {
            self.presses(Key::Character(character));
        }
    }

    /// The app puts `message` in its box by itself, with its cursor at the
    /// end, and draws: a message called back, or a word completed.
    pub fn holds(&mut self, message: &str) {
        self.message.clear();
        self.cursor = 0;
        for character in message.chars() {
            self.insert(character);
        }
        self.draws();
    }

    fn insert(&mut self, character: char) {
        if takes_no_cell(character) && self.cursor > 0 {
            self.message[self.cursor - 1].push(character);
        } else {
            self.message.insert(self.cursor, character.to_string());
            self.cursor += 1;
        }
    }

    fn handle(&mut self, key: Key) {
        // The line the cursor is on: from after the line break before it
        // to the next one.
        let line_start = self.message[..self.cursor]
            .iter()
            .rposition(|character| character == "\n")
            .map_or(0, |at| at + 1);
        let line_end = self.message[self.cursor..]
            .iter()
            .position(|character| character == "\n")
            .map_or(self.message.len(), |at| self.cursor + at);
        match key {
            Key::Character(character) => self.insert(character),
            Key::LineBreak => self.insert('\n'),
            Key::Paste(text) => {
                for character in text.chars() {
                    self.insert(if character == '\r' { '\n' } else { character });
                }
            }
            Key::Left | Key::ShiftLeft => self.cursor = self.cursor.saturating_sub(1),
            Key::Right | Key::ShiftRight => self.cursor = (self.cursor + 1).min(self.message.len()),
            Key::CtrlLeft => {
                // Over the spaces before the cursor, to the start of the
                // word before them.
                let is_word = |at: &usize| !matches!(self.message[*at].as_str(), " " | "\n");
                let word_end = (0..self.cursor).rev().find(is_word).map_or(0, |at| at + 1);
                self.cursor = (0..word_end)
                    .rev()
                    .find(|at| !is_word(at))
                    .map_or(0, |at| at + 1);
            }
            Key::CtrlRight => {
                // Over the spaces after the cursor, to the end of the word
                // after them.
                let end = self.message.len();
                let is_word = |at: &usize| !matches!(self.message[*at].as_str(), " " | "\n");
                let word_start = (self.cursor..end).find(is_word).unwrap_or(end);
                self.cursor = (word_start..end).find(|at| !is_word(at)).unwrap_or(end);
            }
            Key::Home => self.cursor = line_start,
            Key::End => self.cursor = line_end,
            Key::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.message.remove(self.cursor);
                }
            }
            Key::Delete => {
                if self.cursor < self.message.len() {
                    self.message.remove(self.cursor);
                }
            }
            Key::DeleteWord => {
                // The spaces before the cursor, and the word before them.
                let is_space = |at: &usize| self.message[*at] == " ";
                let word_end = (0..self.cursor)
                    .rev()
                    .find(|at| !is_space(at))
                    .map_or(0, |at| at + 1);
                let word_start = (line_start..word_end)
                    .rev()
                    .find(is_space)
                    .map_or(line_start, |at| at + 1);
                self.message.drain(word_start..self.cursor);
                self.cursor = word_start;
            }
            Key::DeleteToEnd => {
                self.message.drain(self.cursor..line_end);
            }
            // The message is sent, and the box is empty again.
            Key::Enter => {
                self.message.clear();
                self.cursor = 0;
            }
            Key::Up | Key::Down | Key::Escape | Key::Other => {}
        }
    }

    /// The rows the app wraps the message onto: for each, the places in the
    /// message where it starts and ends, and whether a line ends with it.
    fn wrapped(&self) -> Vec<(usize, usize, bool)> {
        let width = self.cols - 4;
        let is = |at: usize, what: &str| self.message[at] == what;
        let mut rows = Vec::new();
        let mut line_start = 0;
        loop {
            let line_end = (line_start..self.message.len())
                .find(|at| is(*at, "\n"))
                .unwrap_or(self.message.len());
            let (mut row_start, mut cells, mut at) = (line_start, 0, line_start);
            while at < line_end {
                if is(at, " ") {
                    cells += 1;
                    at += 1;
                    continue;
                }
                let word_end = (at..line_end).find(|at| is(*at, " ")).unwrap_or(line_end);
                if cells + (word_end - at) <= width {
                    cells += word_end - at;
                    at = word_end;
                    continue;
                }
                if word_end - at > width {
                    // A word longer than a row is cut where the row is
                    // full, and goes on at the start of the next.
                    at += width - cells;
                }
                rows.push((row_start, at, false));
                (row_start, cells) = (at, 0);
            }
            rows.push((row_start, line_end, true));
            if line_end == self.message.len() {
                return rows;
            }
            line_start = line_end + 1;
        }
    }

    /// The row the app's cursor is on and its place on that row, counted in
    /// characters from the row's first.
    fn cursor_in_rows(&self) -> (usize, usize) {
        let rows = self.wrapped();
        let row = rows
            .iter()
            .position(|(start, end, ends_line)| {
                (*start..*end).contains(&self.cursor) || (*ends_line && self.cursor == *end)
            })
            .unwrap();
        (row, self.cursor - rows[row].0)
    }

    /// The rows of the message in Logical order, as the app wrapped them,
    /// without the spaces at the end of each.
    pub fn rows_as_typed(&self) -> Vec<String> {
        self.wrapped()
            .iter()
            .map(|(start, end, _)| self.message[*start..*end].concat().trim_end().to_string())
            .collect()
    }

    /// The Paragraph direction of each row the app wrapped the message
    /// onto: that of the line the row is part of, a line being what stands
    /// between two line breaks. A line takes the direction of its first
    /// letter, and one with no letter in it is left to right, also when
    /// its digits are Farsi ones.
    pub fn directions(&self) -> Vec<Direction> {
        let is_break = |character: &String| character == "\n";
        self.wrapped()
            .iter()
            .map(|(start, _, _)| {
                let line_start = self.message[..*start]
                    .iter()
                    .rposition(is_break)
                    .map_or(0, |at| at + 1);
                let line_end = self.message[*start..]
                    .iter()
                    .position(is_break)
                    .map_or(self.message.len(), |at| start + at);
                let line = self.message[line_start..line_end].concat();
                match line.contains(char::is_alphabetic) {
                    true => direction_of(&line),
                    false => Direction::LeftToRight,
                }
            })
            .collect()
    }

    /// True when the box holds no message, and shows the app's hint.
    pub fn holds_nothing(&self) -> bool {
        self.message.is_empty()
    }

    /// The rows of the message as the app draws them.
    fn rows_as_drawn(&self) -> Vec<String> {
        let (cursor_row, cursor_at) = self.cursor_in_rows();
        self.wrapped()
            .iter()
            .enumerate()
            .map(|(row, (start, end, _))| {
                let mut shown: Vec<&str> = self.message[*start..*end]
                    .iter()
                    .map(String::as_str)
                    .collect();
                let text = shown
                    .iter()
                    .rposition(|character| *character != " ")
                    .map_or(0, |last| last + 1);
                // The spaces at the end are drawn only up to the cursor.
                let through = match row == cursor_row {
                    true => text.max(cursor_at + 1),
                    false => text,
                };
                shown.resize(through, " ");
                as_drawn(&shown.concat())
            })
            .collect()
    }

    /// The app draws its screen: the box and the line under it.
    pub fn draws(&mut self) {
        let border = format!("\x1b[38;2;136;136;136m{}\x1b[39m", "─".repeat(self.cols));
        let mut frame = String::from("\x1b[?2026h\x1b[?25l");
        let mut row = TOP;
        let mut draw = |frame: &mut String, text: &str| {
            frame.push_str(&format!("\x1b[{};1H\x1b[2K{text}", row + 1));
            row += 1;
        };
        draw(&mut frame, &border);
        if self.message.is_empty() {
            draw(
                &mut frame,
                &format!("{}\x1b[2m{}\x1b[22m", self.mark, self.hint),
            );
        } else {
            for (row, drawn) in self.rows_as_drawn().iter().enumerate() {
                let margin = if row == 0 { self.mark.as_str() } else { "  " };
                draw(&mut frame, &format!("{margin}{drawn}"));
            }
        }
        draw(&mut frame, &border);
        draw(
            &mut frame,
            "  \x1b[38;2;153;153;153m⏵⏵ auto mode on (shift+tab to cycle)\x1b[39m",
        );
        frame.push_str("\r\n\x1b[J");
        let cursor = self.cursor_drawn();
        frame.push_str(&format!("\x1b[{};{}H", cursor.row + 1, cursor.col + 1));
        if self.shows_cursor {
            frame.push_str("\x1b[?25h");
        }
        frame.push_str("\x1b[?2026l");
        self.run.app_writes(frame);
    }

    /// Where the app puts the Terminal's cursor.
    fn cursor_drawn(&self) -> Position {
        let (row, in_row) = match self.message.is_empty() {
            true => (0, 0),
            false => self.cursor_in_rows(),
        };
        at((TOP + 1 + row) as u16, (2 + in_row) as u16)
    }

    /// The rows of the screen the message is on.
    fn rows_of_the_box(&self) -> std::ops::Range<usize> {
        let rows = match self.message.is_empty() {
            true => 1,
            false => self.wrapped().len(),
        };
        TOP + 1..TOP + 1 + rows
    }

    /// The rows of the message as the Terminal shows them.
    pub fn shown(&self) -> Vec<String> {
        self.run.terminal.screen_text()[self.rows_of_the_box()].to_vec()
    }

    /// The rows of the message as the app drew them.
    pub fn drawn(&self) -> Vec<String> {
        self.run.without_farcli.screen_text()[self.rows_of_the_box()].to_vec()
    }

    /// Where the Terminal's cursor is, counted from the message's first
    /// row.
    pub fn cursor(&self) -> Position {
        let cursor = self.run.terminal.cursor();
        at(cursor.row - (TOP + 1) as u16, cursor.col)
    }

    /// The character the Terminal's cursor is on.
    pub fn character_under_the_cursor(&self) -> char {
        self.run.terminal.cell(self.run.terminal.cursor()).character
    }

    /// True when the Terminal shows every cell of the screen, and the
    /// cursor, as the app drew them.
    pub fn is_shown_as_drawn(&self) -> bool {
        self.run.screen_is_as_drawn()
    }
}

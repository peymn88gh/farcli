//! Input box: shows the message the user is typing as a right-to-left text
//! field, and puts the cursor where the next character will appear.
//!
//! This is for a Target app that puts Farsi into Screen order itself, row
//! by row, as Claude Code does on Windows. Such an app's Input box does
//! not always say what the message is: one row on screen can be what the
//! app draws for more than one order of the same characters (see the Undo
//! step). The message has two witnesses here, and each is used for what it
//! knows:
//!
//! - **The screen** says which characters are on which row, and where the
//!   app's cursor is. Nothing is ever shown that the screen does not hold.
//! - **The keys** the user pressed say in which order the characters were
//!   typed. From them farcli keeps the message it believes the box holds.
//!
//! Each time the app has drawn, the message believed is checked against the
//! screen: the app's own way of drawing it, worked out row by row, has to
//! give exactly the rows on screen. Then the rows are laid out from it.
//! When it does not (the user called back an earlier message, the app
//! completed a word, a key did something farcli does not follow) the
//! message is read from the screen alone, which works when every row has
//! only one possible order. When some row has more than one, nobody knows
//! the message, and the box is shown as the app drew it.
//!
//! The app is often behind the keys: it draws for other reasons while keys
//! are on their way to it. So the keys the screen has not borne out yet are
//! kept apart from the message it has, and each frame is matched against
//! the message as it was after each of them.
//!
//! The message believed also says what an arrow key means. The user means
//! a side of the screen, and the app moves its cursor in Logical order. So
//! while the box is shown as a text field, the app is sent the arrow keys
//! that take its cursor to the cell beside the one it is on
//! (`editing_keys`). Every other key is forwarded as it came.

mod editing_keys;

use std::ops::Range;

use self::editing_keys::{Arrow, FieldRow, Reach, Side};
use crate::layout::{self, Bearing, Direction};
use crate::screen::{Cell, CellWidth, Row};
use crate::terminal_input::{Input, Key};
use crate::undo;

/// What farcli knows of the message in a Target app's Input box.
pub(crate) struct InputBox {
    /// The message as the screen last bore it out, in Logical order.
    known: Option<Message>,
    /// What the user has done since that the screen has not shown yet, in
    /// the order it was done.
    waiting: Vec<Edit>,
    /// What the Terminal sends now is pasted text.
    pasting: bool,
    /// The rows of the message as farcli last showed them, with the app's
    /// cursor among them. `None` while the box is shown as the app drew it,
    /// shows no cursor or is not on screen: then every key is forwarded as
    /// it was pressed.
    shown: Option<Vec<ShownRow>>,
    /// What became of the Left key and of the Right key since each last
    /// came up.
    arrows_down: [ArrowDown; 2],
}

/// One row of the message as farcli showed it.
#[derive(Debug, Clone, Copy)]
struct ShownRow {
    /// The place in the message of the row's first character.
    start: usize,
    /// The direction the row was laid out in.
    direction: Direction,
}

/// What became of an arrow key each time it went down since it last came
/// up: a key held down goes down many times and comes up once.
#[derive(Debug, Clone, Copy, Default)]
struct ArrowDown {
    /// It was forwarded as pressed, so the app is still to see it come up.
    forwarded: bool,
    /// The app was sent other keys in its place, each going down and
    /// coming up.
    replaced: bool,
}

/// Where the Input box's message is on the screen.
pub(crate) struct Place<'a> {
    /// The rows the message is drawn on.
    pub rows: &'a mut [Row],
    /// The columns of those rows it is drawn in.
    pub cols: Range<usize>,
    /// How many cells of a row the app fills before it wraps.
    pub wrap_width: usize,
    /// The app's cursor, when it is among the message's rows and columns:
    /// the row counted from the message's first, the column from the
    /// message's first column.
    pub cursor: Option<(usize, usize)>,
    /// The box shows no message: nothing, or the app's own hint.
    pub is_empty: bool,
}

/// More keys than the screen is ever behind. With more than this waiting,
/// the message is no longer followed by its keys.
const MOST_WAITING: usize = 1024;

/// With no more waiting than this, a frame that matches nothing is also
/// tried against the message without the oldest of them: keys that went to
/// something else the app was showing, and never to the box.
const MOST_WAITING_TRIED_WITHOUT_THE_OLDEST: usize = 64;

/// A message longer than this is not searched for a stretch that a key may
/// have deleted.
const LONGEST_TRIED_FOR_A_DELETION: usize = 2000;

/// When the app's cursor may not have been where farcli believed, the keys
/// waiting are tried from every place in the message, for as many of them
/// as the app may have handled. No more tries than this are made for one
/// frame: a few keys in a long message, or many in a short one.
const MOST_PLACES_TRIED_FROM: usize = 20_000;

/// One thing the user did to the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edit {
    /// Typed or pasted a character.
    Insert(char),
    /// Broke the line.
    LineBreak,
    /// Deleted the character before the cursor.
    Backspace,
    /// Deleted the character at the cursor.
    Delete,
    /// Moved the cursor one character back, towards the start of the
    /// message.
    Back,
    /// Moved the cursor one character on, towards its end.
    On,
    /// Pressed a key farcli does not follow. It may have moved the cursor
    /// anywhere, and if it changed the message the screen will show it.
    Other,
}

/// One character of the message, or a break between two rows of it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Unit {
    /// A character and whatever rides on it: a Half-space, a vowel mark.
    Character(char, Vec<char>),
    /// A line break the user typed or pasted.
    LineBreak,
    /// The end of a row read from the screen, which does not say whether
    /// the user broke the line there or the app wrapped it at a space.
    RowEnd,
}

impl Unit {
    fn is_space(&self) -> bool {
        matches!(self, Unit::Character(' ', marks) if marks.is_empty())
    }
}

/// The message farcli believes the Input box holds.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Message {
    units: Vec<Unit>,
    /// Where the next character goes: before the unit at this place.
    cursor: usize,
    /// The cursor has not been seen on screen since a key that may have
    /// moved it anywhere.
    cursor_is_a_guess: bool,
}

impl Message {
    /// Does `edit` to the message. False when the result cannot be told.
    fn apply(&mut self, edit: Edit) -> bool {
        // A key that changes the message does so at the cursor. With the
        // cursor a guess, what it leaves is a guess too.
        if self.cursor_is_a_guess && !matches!(edit, Edit::Back | Edit::On | Edit::Other) {
            return false;
        }
        match edit {
            Edit::Insert(character) if layout::takes_no_cell(character) => {
                // It rides on the character before the cursor.
                match self.cursor.checked_sub(1).map(|at| &mut self.units[at]) {
                    Some(Unit::Character(_, marks)) => marks.push(character),
                    _ => return false,
                }
            }
            Edit::Insert(character) => {
                self.units
                    .insert(self.cursor, Unit::Character(character, Vec::new()));
                self.cursor += 1;
            }
            Edit::LineBreak => {
                self.units.insert(self.cursor, Unit::LineBreak);
                self.cursor += 1;
            }
            Edit::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.units.remove(self.cursor);
                }
            }
            Edit::Delete => {
                if self.cursor < self.units.len() {
                    self.units.remove(self.cursor);
                }
            }
            Edit::Back => self.cursor = self.cursor.saturating_sub(1),
            Edit::On => self.cursor = (self.cursor + 1).min(self.units.len()),
            Edit::Other => self.cursor_is_a_guess = true,
        }
        true
    }

    /// How many of its characters show on a screen.
    fn characters_that_show(&self) -> usize {
        self.units
            .iter()
            .filter(|unit| matches!(unit, Unit::Character(..)) && !unit.is_space())
            .count()
    }
}

impl InputBox {
    pub fn new() -> InputBox {
        InputBox {
            known: None,
            waiting: Vec::new(),
            pasting: false,
            shown: None,
            arrows_down: [ArrowDown::default(); 2],
        }
    }

    /// The Terminal sent `input`. Returns what the Target app is to be sent
    /// in its place, or `None` when it is to be sent `input` as it came.
    ///
    /// Only the Left and Right arrow keys are ever replaced, and only while
    /// the box is shown as a text field: the user means a side of the
    /// screen, and the app moves its cursor in Logical order. With
    /// `shown_as_drawn`, the user is looking at the app's own screen
    /// whatever farcli makes of it, and every key is forwarded as it came.
    pub fn user_typed(&mut self, input: &Input, shown_as_drawn: bool) -> Option<Vec<Input>> {
        let Some(arrow) = Arrow::read(input) else {
            self.follow(input);
            return None;
        };
        let key = match arrow.side {
            Side::Left => 0,
            Side::Right => 1,
        };
        if !arrow.down {
            // The app has seen every key sent in this one's place come up
            // already.
            let down = std::mem::take(&mut self.arrows_down[key]);
            return (down.replaced && !down.forwarded).then(Vec::new);
        }
        let mut presses: Vec<Side> = Vec::new();
        match arrow.reach {
            Reach::Character => {
                for _ in 0..arrow.repeats {
                    let (side, times) = match shown_as_drawn {
                        true => (arrow.side, 1),
                        false => self.keys_towards(arrow.side),
                    };
                    presses.extend(std::iter::repeat_n(side, times));
                    let edit = match side {
                        Side::Left => Edit::Back,
                        Side::Right => Edit::On,
                    };
                    self.wait_for(edit, times);
                }
            }
            // The app goes to the start of the word before its cursor or to
            // the end of the word after it. On a row laid out right to
            // left, what comes after is on the left.
            Reach::Word => {
                let side = match self.direction_at_the_cursor() {
                    Some(Direction::RightToLeft) if !shown_as_drawn => arrow.side.other(),
                    _ => arrow.side,
                };
                presses.extend(std::iter::repeat_n(side, arrow.repeats));
                self.wait_for(Edit::Other, 1);
            }
        }
        // Only a key in the Windows form comes up again.
        let comes_up = matches!(input, Input::Key(_));
        if presses.len() == arrow.repeats && presses.iter().all(|side| *side == arrow.side) {
            self.arrows_down[key].forwarded |= comes_up;
            return None;
        }
        self.arrows_down[key].replaced |= comes_up;
        Some(
            presses
                .into_iter()
                .flat_map(|side| Arrow::press(input, side))
                .collect(),
        )
    }

    /// The app's arrow key, and how many presses of it, that take the
    /// cursor one place towards `side` on screen. No press at all when
    /// there is no place on that side and the app's own key would move the
    /// cursor.
    fn keys_towards(&self, side: Side) -> (Side, usize) {
        let Some(expected) = self.expected() else {
            return (side, 1);
        };
        let Some(from) = expected.cursor() else {
            // Where the cursor is will not be known until the app draws.
            // In a message that is right to left on every row, the other
            // key is the right one almost everywhere.
            return match expected.one_direction() {
                Some(Direction::RightToLeft) => (side.other(), 1),
                _ => (side, 1),
            };
        };
        let rows = expected.field();
        let place = |(row, on_row): (usize, usize)| expected.rows[row].start + on_row;
        match editing_keys::beside(&rows, from, side) {
            Some(to) if place(to) < place(from) => (Side::Left, place(from) - place(to)),
            Some(to) => (Side::Right, place(to) - place(from)),
            // Nothing is on that side: the cursor is at the edge of the
            // first row or of the last. At the start or the end of the
            // message the app is sent its key that would go on the same
            // way, which it does nothing for there. Anywhere else that key
            // would move its cursor: the first row of a Farsi message that
            // starts with a number has the number's last digit at its
            // edge, not its first.
            None => {
                let cursor = place(from);
                match editing_keys::leads_onwards(rows[from.0].direction, side) {
                    true if cursor == expected.message.units.len() => (Side::Right, 1),
                    false if cursor == 0 => (Side::Left, 1),
                    _ => (side, 0),
                }
            }
        }
    }

    /// The direction of the row the cursor is on, as far as it can be told
    /// before the app has drawn.
    fn direction_at_the_cursor(&self) -> Option<Direction> {
        let expected = self.expected()?;
        match expected.cursor() {
            Some((row, _)) => Some(expected.rows[row].direction),
            None => expected.one_direction(),
        }
    }

    /// The message as it will be when the app has handled every key it was
    /// sent, as far as that can be told, on the rows farcli last showed it
    /// on. `None` while the box is not shown as a text field.
    fn expected(&self) -> Option<Expected> {
        let mut message = self.known.clone()?;
        let mut rows = self.shown.clone()?;
        let mut rows_are_certain = true;
        for edit in &self.waiting {
            let (length, cursor) = (message.units.len(), message.cursor);
            let ends_a_row =
                |at: usize| !matches!(message.units.get(at), Some(Unit::Character(..)));
            // A row that begins or ends: which characters the app then
            // puts on which row is not known until it draws.
            rows_are_certain &= match edit {
                Edit::LineBreak | Edit::Other => false,
                Edit::Backspace => cursor == 0 || !ends_a_row(cursor - 1),
                Edit::Delete => cursor == length || !ends_a_row(cursor),
                Edit::Insert(_) | Edit::Back | Edit::On => true,
            };
            // A key whose doing cannot be told leaves the message as it
            // was, here, and the rows no longer certain.
            rows_are_certain &= message.apply(*edit);
            // A character typed or deleted is taken to stay on the row the
            // cursor was on, and the rows after it to hold what they held.
            match message.units.len().cmp(&length) {
                std::cmp::Ordering::Greater => rows
                    .iter_mut()
                    .filter(|row| row.start > cursor)
                    .for_each(|row| row.start += 1),
                std::cmp::Ordering::Less => rows
                    .iter_mut()
                    .filter(|row| row.start > message.cursor)
                    .for_each(|row| row.start -= 1),
                std::cmp::Ordering::Equal => {}
            }
        }
        Some(Expected {
            message,
            rows,
            rows_are_certain,
        })
    }

    /// The Terminal sent `input`, which is on its way to the Target app as
    /// it came: what it does to the message.
    fn follow(&mut self, input: &Input) {
        let (edit, times) = match input {
            Input::PasteStarts => {
                self.pasting = true;
                return;
            }
            Input::PasteEnds => {
                self.pasting = false;
                return;
            }
            Input::Focus { .. } => return,
            Input::Character(character) => (self.typed(*character), 1),
            Input::Key(key) if !key.down => return,
            Input::Key(key) => match self.pressed(key) {
                Some(edit) => (edit, usize::from(key.repeats.clamp(1, 64))),
                None => return,
            },
            Input::Other(code) => (
                match code.as_slice() {
                    // Delete as Terminals send it that do not send keys in
                    // the Windows form.
                    b"\x1b[3~" => Edit::Delete,
                    _ => Edit::Other,
                },
                1,
            ),
            Input::Mouse(_) => (Edit::Other, 1),
        };
        self.wait_for(edit, times);
    }

    /// The app was sent a key that does `edit`, `times` over, and has not
    /// drawn what came of it yet.
    fn wait_for(&mut self, edit: Edit, times: usize) {
        if self.known.is_none() {
            return;
        }
        for _ in 0..times {
            // One key farcli does not follow after another says no more
            // than the first did.
            if edit == Edit::Other && self.waiting.last() == Some(&Edit::Other) {
                break;
            }
            self.waiting.push(edit);
        }
        if self.waiting.len() > MOST_WAITING {
            self.known = None;
            self.waiting.clear();
        }
    }

    /// What a character that came from the Terminal does to the message.
    fn typed(&self, character: char) -> Edit {
        match character {
            // Between the marks of a paste the end of a line is a line
            // break. Typed, it is the Enter key, which sends the message.
            '\r' if self.pasting => Edit::LineBreak,
            // The line feed is what Ctrl+J types: a line break.
            '\n' => Edit::LineBreak,
            '\u{8}' | '\u{7f}' => Edit::Backspace,
            character if character.is_control() => Edit::Other,
            character => Edit::Insert(character),
        }
    }

    /// What a key going down does to the message, or `None` for a key that
    /// does nothing by itself.
    fn pressed(&self, key: &Key) -> Option<Edit> {
        // Windows' numbers for the keys.
        const BACKSPACE: u16 = 8;
        const TAB: u16 = 9;
        const ENTER: u16 = 13;
        const SHIFT_CTRL_ALT: std::ops::RangeInclusive<u16> = 16..=18;
        const CAPS_LOCK: u16 = 20;
        const ESCAPE: u16 = 27;
        const PAGE_UP_TO_DOWN_ARROW: std::ops::RangeInclusive<u16> = 33..=40;
        const INSERT: u16 = 45;
        const DELETE: u16 = 46;
        const WINDOWS_KEYS: std::ops::RangeInclusive<u16> = 91..=92;
        const FUNCTION_KEYS: std::ops::RangeInclusive<u16> = 112..=135;
        const NUM_LOCK_AND_SCROLL_LOCK: std::ops::RangeInclusive<u16> = 144..=145;
        // Which of Alt, Ctrl and Shift are held, as Windows numbers them.
        const ALT: u32 = 0x1 | 0x2;
        const CTRL: u32 = 0x4 | 0x8;
        const SHIFT: u32 = 0x10;
        let held = key.modifiers & (ALT | CTRL | SHIFT);

        let edit = match key.key {
            number
                if SHIFT_CTRL_ALT.contains(&number)
                    || WINDOWS_KEYS.contains(&number)
                    || NUM_LOCK_AND_SCROLL_LOCK.contains(&number)
                    || number == CAPS_LOCK =>
            {
                return None
            }
            ENTER if self.pasting => Edit::LineBreak,
            BACKSPACE if held == 0 => Edit::Backspace,
            DELETE if held == 0 => Edit::Delete,
            number
                if PAGE_UP_TO_DOWN_ARROW.contains(&number)
                    || FUNCTION_KEYS.contains(&number)
                    || matches!(number, BACKSPACE | TAB | ENTER | ESCAPE | INSERT | DELETE) =>
            {
                Edit::Other
            }
            // A character typed with Alt held, and Ctrl not, is a shortcut
            // of the app's. With both held it is a character the keyboard
            // has on its third level.
            _ if key.modifiers & ALT != 0 && key.modifiers & CTRL == 0 => Edit::Other,
            _ => match char::from_u32(key.character.into()) {
                Some('\0') | None => Edit::Other,
                Some(character) => self.typed(character),
            },
        };
        Some(edit)
    }

    /// Lays out the message in `place` as a text field, in place, and
    /// returns the column of `place` where the Terminal's cursor belongs
    /// when that is not where the app has it. Leaves everything as the app
    /// drew it when the message cannot be told.
    pub fn correct(&mut self, place: Place) -> Option<usize> {
        self.shown = None;
        let shows_the_apps_hint = place.is_empty
            && place.rows.iter().any(|row| {
                let mut cells = row[place.cols.clone()].iter();
                cells.any(|cell| cell.character != ' ' && !cell.is_left_alone())
            });
        if shows_the_apps_hint {
            // The app shows its hint only in a box that holds nothing, so
            // the message believed is gone whatever became of it: spaces
            // at its end that nothing on screen ever showed, too. Keys
            // typed since are still on their way.
            self.known = Some(Message {
                units: Vec::new(),
                cursor: 0,
                cursor_is_a_guess: false,
            });
        }
        // The hint is no part of the message: such a box is one empty row.
        // A box that only looks empty may hold spaces and line breaks, and
        // is read like any other: its cursor says how many spaces are
        // before it.
        let empty = [];
        let screen: Vec<Drawn> = if shows_the_apps_hint {
            vec![Drawn::of(&empty)]
        } else {
            place
                .rows
                .iter()
                .map(|row| Drawn::of(&row[place.cols.clone()]))
                .collect()
        };
        let cursor = place.cursor.filter(|_| !shows_the_apps_hint);
        let reading = self.read(&screen, cursor, place.wrap_width)?;
        let message = self.known.as_ref()?;
        let directions = directions(&message.units, &reading, &screen, place.wrap_width);

        let width = place.cols.len();
        let mut laid_out: Vec<(usize, layout::Line)> = Vec::new();
        for (row, read) in reading.rows.iter().enumerate() {
            let cells: Vec<Cell> = read
                .logical
                .iter()
                .flat_map(|unit| screen[row].units[*unit].clone())
                .map(|cell| match screen[row].cells[cell].is_left_alone() {
                    true => Cell::blank(),
                    false => screen[row].cells[cell].clone(),
                })
                .collect();
            // A left-to-right row with no Farsi in it is already what
            // Layout would make of it.
            if directions[row] == Direction::LeftToRight && !layout::holds_right_to_left(&cells) {
                continue;
            }
            if cells.len() > width {
                return None;
            }
            laid_out.push((row, layout::lay_out_line(&cells, directions[row], width)));
        }
        drop(screen);
        if reading.cursor.is_some() {
            let rows = reading.rows.iter().zip(&directions);
            self.shown = Some(
                rows.map(|(read, direction)| ShownRow {
                    start: read.start,
                    direction: *direction,
                })
                .collect(),
            );
        }
        let mut cursor_col = None;
        for (row, line) in laid_out {
            if let Some((_, col)) = cursor.filter(|(cursor_row, _)| *cursor_row == row) {
                cursor_col = Some(line.screen_col(col));
            }
            place.rows[row][place.cols.clone()].clone_from_slice(&line.cells);
        }
        cursor_col
    }

    /// The app has drawn a frame with no Input box that farcli shows as a
    /// text field. Keys are forwarded as pressed until it draws one.
    pub fn is_out_of_sight(&mut self) {
        self.shown = None;
    }

    /// Works out which message the screen shows and how each of its rows
    /// reads, and keeps the message. `None` when it cannot be told.
    fn read(
        &mut self,
        screen: &[Drawn],
        cursor: Option<(usize, usize)>,
        wrap_width: usize,
    ) -> Option<Reading> {
        if let Some((message, reading, borne_out)) = self.believed(screen, cursor, wrap_width) {
            self.waiting.drain(..borne_out);
            self.keep(message, &reading);
            return Some(reading);
        }
        // The keys do not explain the screen. It is its only witness now.
        let units = read_from_screen(screen)?;
        let reading = read_as(&units, screen, cursor, wrap_width)?;
        self.waiting.clear();
        self.keep(
            Message {
                units,
                cursor: 0,
                cursor_is_a_guess: true,
            },
            &reading,
        );
        Some(reading)
    }

    /// Keeps `message`, which the screen has borne out with `reading`.
    fn keep(&mut self, mut message: Message, reading: &Reading) {
        for at in &reading.spaces {
            message.units[*at] = Unit::Character(' ', Vec::new());
        }
        if let Some(cursor) = reading.cursor {
            message.cursor = cursor;
            message.cursor_is_a_guess = false;
        }
        message.cursor = message.cursor.min(message.units.len());
        self.known = Some(message);
    }

    /// The message believed, as it was after the latest of the keys waiting
    /// that the screen bears out, with how many of them that is.
    fn believed(
        &self,
        screen: &[Drawn],
        cursor: Option<(usize, usize)>,
        wrap_width: usize,
    ) -> Option<(Message, Reading, usize)> {
        let known = self.known.as_ref()?;
        let on_screen: usize = screen.iter().map(Drawn::characters_that_show).sum();
        let borne_out = |message: &Message| -> Option<(Reading, bool)> {
            if message.characters_that_show() != on_screen {
                return None;
            }
            let reading = read_as(&message.units, screen, cursor, wrap_width)?;
            let cursor_agrees = message.cursor_is_a_guess
                || reading.cursor.is_none()
                || reading.cursor == Some(message.cursor);
            Some((reading, cursor_agrees))
        };

        // The message after each of the keys waiting, oldest first. The
        // latest that the screen bears out, cursor and all, is the one the
        // app has drawn. Failing that, the latest whose text it bears out
        // and in which nothing but the cursor has moved.
        //
        // A key that changes the message is borne out only with the cursor
        // where that key leaves it. One row on screen is what the app
        // draws for more than one message: a letter typed before a number
        // is drawn as the same letter typed after it, and a space typed or
        // deleted at the end of a row is not drawn at all. Only the cursor
        // says where such a key was done, or that it was.
        let mut best: Option<(Message, Reading, usize, bool)> = None;
        let mut message = known.clone();
        let mut only_the_cursor_says = false;
        for done in 0..=self.waiting.len() {
            let edit = done.checked_sub(1).map(|last| self.waiting[last]);
            if edit.is_some_and(|edit| !message.apply(edit)) {
                break;
            }
            only_the_cursor_says |=
                !matches!(edit, None | Some(Edit::Back | Edit::On | Edit::Other));
            let Some((reading, cursor_agrees)) = borne_out(&message) else {
                continue;
            };
            let is_better = best.as_ref().is_none_or(|(.., agreed)| !agreed);
            if cursor_agrees || (is_better && !only_the_cursor_says) {
                best = Some((message.clone(), reading, done, cursor_agrees));
            }
        }
        if let Some((message, reading, done, _)) = best {
            return Some((message, reading, done));
        }
        if let Some(believed) = self.done_from_another_place(known, &borne_out) {
            return Some(believed);
        }
        if let Some(believed) = self.after_a_deletion(known, &borne_out) {
            return Some(believed);
        }

        // Keys that went to something else the app was showing never
        // reached the box: the message without the oldest of them.
        if self.waiting.len() > MOST_WAITING_TRIED_WITHOUT_THE_OLDEST {
            return None;
        }
        'without: for dropped in 1..self.waiting.len() {
            let mut message = known.clone();
            for edit in &self.waiting[dropped..] {
                if !message.apply(*edit) {
                    continue 'without;
                }
            }
            if let Some((reading, true)) = borne_out(&message) {
                return Some((message, reading, self.waiting.len()));
            }
        }
        None
    }

    /// The message believed, if the app's cursor was not where farcli
    /// believed it when it handled the keys waiting. That happens after a
    /// key farcli does not follow that moved the cursor, when keys that
    /// change the message came after it before the app drew: Home and a
    /// letter, a word jump and Backspace. It also happens after arrow keys
    /// of which the app had handled fewer, when it drew, than farcli took
    /// it to have.
    ///
    /// What the app drew is the message with those keys done from some one
    /// place, as many of them as it has handled, and its cursor where they
    /// leave it, or it is not. Returns how many of the keys waiting that
    /// bears out.
    fn done_from_another_place(
        &self,
        known: &Message,
        borne_out: &dyn Fn(&Message) -> Option<(Reading, bool)>,
    ) -> Option<(Message, Reading, usize)> {
        // Up to the last key farcli does not follow, the keys were done
        // where farcli believes. That key may have left the cursor
        // anywhere.
        let from = self
            .waiting
            .iter()
            .rposition(|edit| *edit == Edit::Other)
            .map_or(0, |key| key + 1);
        let mut before = known.clone();
        for edit in &self.waiting[..from] {
            if !before.apply(*edit) {
                return None;
            }
        }
        let after = self.waiting.len() - from;
        if after * (before.units.len() + 1) > MOST_PLACES_TRIED_FROM {
            return None;
        }
        before.cursor_is_a_guess = false;
        // The most keys first: the app has usually handled them all.
        for done in (from + 1..=self.waiting.len()).rev() {
            'places: for cursor in 0..=before.units.len() {
                let mut message = before.clone();
                message.cursor = cursor;
                for edit in &self.waiting[from..done] {
                    if !message.apply(*edit) {
                        continue 'places;
                    }
                }
                match borne_out(&message) {
                    Some((reading, _))
                        if reading.cursor.is_none_or(|cursor| cursor == message.cursor) =>
                    {
                        return Some((message, reading, done))
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// The message believed, if the last key farcli does not follow
    /// deleted what stood on one side of the cursor: the word before it,
    /// the line up to it, the rest of the line. Which key deletes what is
    /// the app's business. What it drew is the message with one stretch
    /// next to the cursor gone, and as many of the keys after that one done
    /// as it has handled, or it is not. Returns how many of the keys
    /// waiting that bears out.
    fn after_a_deletion(
        &self,
        known: &Message,
        borne_out: &dyn Fn(&Message) -> Option<(Reading, bool)>,
    ) -> Option<(Message, Reading, usize)> {
        let key = self.waiting.iter().rposition(|edit| *edit == Edit::Other)?;
        let mut before = known.clone();
        for edit in &self.waiting[..key] {
            if !before.apply(*edit) {
                return None;
            }
        }
        if before.units.len() > LONGEST_TRIED_FOR_A_DELETION {
            return None;
        }
        // All the keys after it first: the app has usually handled them
        // all. In a long message with many keys waiting, only that.
        let after = self.waiting.len() - (key + 1);
        let fewest = match after * (before.units.len() + 1) > MOST_PLACES_TRIED_FROM {
            true => self.waiting.len(),
            false => key + 1,
        };
        let cursor = before.cursor;
        // Spaces at the end of a line are not drawn, so the screen cannot
        // say whether a stretch after the cursor took them along. The key
        // that deletes the rest of a line does, and it is the usual one:
        // a stretch that reaches them takes them.
        let with_the_spaces_at_the_end_of_the_line = |to: usize| -> usize {
            let spaces = before.units[to..].iter().take_while(|unit| unit.is_space());
            let end = to + spaces.count();
            match before.units.get(end) {
                None | Some(Unit::LineBreak | Unit::RowEnd) => end,
                Some(Unit::Character(..)) => to,
            }
        };
        for done in (fewest..=self.waiting.len()).rev() {
            // The shortest stretch first: before the cursor, then after it.
            let before_the_cursor = (0..cursor).rev().map(|from| from..cursor);
            let after_the_cursor = (cursor + 1..=before.units.len())
                .map(|to| cursor..with_the_spaces_at_the_end_of_the_line(to));
            'stretches: for gone in before_the_cursor.chain(after_the_cursor) {
                let mut message = before.clone();
                message.cursor = gone.start;
                message.cursor_is_a_guess = false;
                message.units.drain(gone);
                for edit in &self.waiting[key + 1..done] {
                    if !message.apply(*edit) {
                        continue 'stretches;
                    }
                }
                // The app's cursor has to be where the deletion, and the
                // keys after it, left it.
                match borne_out(&message) {
                    Some((reading, _)) if reading.cursor == Some(message.cursor) => {
                        return Some((message, reading, done))
                    }
                    _ => {}
                }
            }
        }
        None
    }
}

/// The message as it will be when the app has handled every key it was
/// sent.
struct Expected {
    message: Message,
    /// The rows it is on, as far as that can be told before the app draws.
    rows: Vec<ShownRow>,
    /// False when a key waiting begins or ends a row, or does something
    /// farcli does not follow.
    rows_are_certain: bool,
}

impl Expected {
    /// The row the cursor is on and its place on that row, when both are
    /// known.
    fn cursor(&self) -> Option<(usize, usize)> {
        if !self.rows_are_certain || self.message.cursor_is_a_guess {
            return None;
        }
        let cursor = self.message.cursor;
        let row = self.rows.iter().rposition(|row| row.start <= cursor)?;
        Some((row, cursor - self.rows[row].start))
    }

    /// The direction of every row, when they all have the same.
    fn one_direction(&self) -> Option<Direction> {
        let first = self.rows.first()?.direction;
        self.rows
            .iter()
            .all(|row| row.direction == first)
            .then_some(first)
    }

    /// The rows as a text field: the places the cursor can be on each.
    fn field(&self) -> Vec<FieldRow> {
        let units = &self.message.units;
        let rows = self.rows.iter().enumerate();
        rows.map(|(at, row)| {
            let end = self.rows.get(at + 1).map_or(units.len(), |next| next.start);
            let end = end.min(units.len());
            let mut characters: Vec<char> = units[row.start.min(end)..end]
                .iter()
                .map(|unit| match unit {
                    Unit::Character(character, _) => *character,
                    // The place at the end of a line.
                    Unit::LineBreak | Unit::RowEnd => ' ',
                })
                .collect();
            if at + 1 == self.rows.len() {
                // The place after the message's last character.
                characters.push(' ');
            }
            FieldRow {
                characters,
                direction: row.direction,
            }
        })
        .collect()
    }
}

/// One row of the Input box's message as the app drew it.
struct Drawn<'a> {
    /// The cells of the columns the message is drawn in.
    cells: &'a [Cell],
    /// The cells of each character, from the left: two for a character two
    /// cells wide.
    units: Vec<Range<usize>>,
}

impl<'a> Drawn<'a> {
    fn of(cells: &'a [Cell]) -> Drawn<'a> {
        let mut units: Vec<Range<usize>> = Vec::with_capacity(cells.len());
        for (position, cell) in cells.iter().enumerate() {
            match units.last_mut() {
                Some(unit) if cell.width == CellWidth::SecondHalf && unit.len() == 1 => {
                    unit.end = position + 1
                }
                _ => units.push(position..position + 1),
            }
        }
        Drawn { cells, units }
    }

    fn cell(&self, unit: usize) -> &Cell {
        &self.cells[self.units[unit].start]
    }

    /// True when nothing is drawn in the unit's cells.
    fn is_blank(&self, unit: usize) -> bool {
        let cell = self.cell(unit);
        (cell.character == ' ' || cell.is_left_alone()) && cell.marks.is_empty()
    }

    fn characters_that_show(&self) -> usize {
        (0..self.units.len())
            .filter(|unit| !self.is_blank(*unit))
            .count()
    }

    /// True when the unit shows `wanted`.
    fn shows(&self, unit: usize, wanted: &Unit) -> bool {
        match wanted {
            Unit::Character(character, marks) if !wanted.is_space() => {
                let cell = self.cell(unit);
                cell.character == *character && cell.marks == *marks
            }
            _ => self.is_blank(unit),
        }
    }

    /// How many units lie wholly before the cell `col`.
    fn units_before(&self, col: usize) -> usize {
        self.units.iter().take_while(|unit| unit.end <= col).count()
    }
}

/// How the rows on screen read as one message.
struct Reading {
    rows: Vec<RowRead>,
    /// The place in the message where the app's cursor is.
    cursor: Option<usize>,
    /// The row ends of the message that turned out to be spaces.
    spaces: Vec<usize>,
}

/// How one row on screen reads.
struct RowRead {
    /// The place in the message of the row's first character.
    start: usize,
    /// The row's units in Logical order, as places in the drawn row: its
    /// text, then the empty cells the app drew after it.
    logical: Vec<usize>,
    /// How many of `logical` are the row's text.
    text: usize,
    end: End,
}

/// What a row of the message ends with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    /// The app wrapped the message here.
    Wrap,
    LineBreak,
    /// A line break or a wrap: the screen does not say.
    Unknown,
    /// The message ends here.
    Message,
}

/// Reads the rows on screen as the message `units`: the app, given this
/// message, must have drawn exactly these rows. `None` when it would not.
///
/// The app wraps the message onto rows of `wrap_width` cells, leaving the
/// space it wrapped at on the end of the row, and draws each row in Screen
/// order by the row's own first letter. It does not draw the spaces at the
/// end of a row, except up to its cursor.
fn read_as(
    units: &[Unit],
    screen: &[Drawn],
    cursor: Option<(usize, usize)>,
    wrap_width: usize,
) -> Option<Reading> {
    let mut reading = Reading {
        rows: Vec::with_capacity(screen.len()),
        cursor: None,
        spaces: Vec::new(),
    };
    let mut at = 0;
    for (row, drawn) in screen.iter().enumerate() {
        let is_last = row + 1 == screen.len();
        let start = at;
        // The row's text runs up to the last character of it that shows.
        let mut to_find = drawn.characters_that_show();
        while to_find > 0 {
            match units.get(at)? {
                Unit::LineBreak => return None,
                // Inside a row, what ended a row before is the space the
                // app wrapped at.
                Unit::RowEnd => reading.spaces.push(at),
                unit if unit.is_space() => {}
                Unit::Character(..) => to_find -= 1,
            }
            at += 1;
        }
        let text = &units[start..at];
        let characters: Vec<char> = text
            .iter()
            .map(|unit| match unit {
                Unit::Character(character, _) => *character,
                _ => ' ',
            })
            .collect();
        let drawn_direction = first_letter(&characters).unwrap_or(Direction::LeftToRight);
        // For each place on screen from the left, the place in the text.
        let order = layout::screen_order(&characters, drawn_direction);
        let shows = |place: usize| !matches!(&text[order[place]], unit if unit.is_space() || *unit == Unit::RowEnd);
        let first_shown = (0..order.len()).find(|place| shows(*place));
        let first_drawn = (0..drawn.units.len()).find(|unit| !drawn.is_blank(*unit));
        // How far right of the area's left edge the app drew the text: a
        // row drawn right to left starts after the spaces at its end.
        let shift = match (first_shown, first_drawn) {
            (Some(shown), Some(on_screen)) => on_screen.checked_sub(shown)?,
            _ => 0,
        };
        if shift > 0 && drawn_direction == Direction::LeftToRight {
            return None;
        }
        if shift + order.len() > drawn.units.len() {
            return None;
        }
        let as_drawn = |place: usize| drawn.shows(shift + place, &text[order[place]]);
        if !(0..order.len()).all(as_drawn) {
            return None;
        }

        // The row in Logical order: its text, then the empty cells the app
        // drew after it. Drawn right to left those are on the left of the
        // text; drawn left to right they cannot be seen, and reach as far
        // as the cursor.
        let mut logical = vec![0; order.len()];
        for (place, in_text) in order.iter().enumerate() {
            logical[*in_text] = shift + place;
        }
        let cursor_here = cursor.filter(|(cursor_row, _)| *cursor_row == row);
        match drawn_direction {
            Direction::RightToLeft => logical.extend((0..shift).rev()),
            Direction::LeftToRight => {
                if let Some((_, col)) = cursor_here {
                    let through = (drawn.units_before(col) + 1).min(drawn.units.len());
                    logical.extend(order.len()..through);
                }
            }
        }

        // The spaces after the text are the row's too, and then whatever
        // ended the row.
        while units.get(at).is_some_and(Unit::is_space) {
            at += 1;
        }
        let after_spaces = at;
        let end = match units.get(at) {
            Some(Unit::LineBreak) if !is_last => {
                at += 1;
                End::LineBreak
            }
            Some(Unit::RowEnd) if !is_last => {
                at += 1;
                End::Unknown
            }
            // With no row below, what ended a row before is a space.
            Some(Unit::RowEnd) => {
                reading.spaces.push(at);
                at += 1;
                while units.get(at).is_some_and(Unit::is_space) {
                    at += 1;
                }
                match units.get(at) {
                    None => End::Message,
                    Some(_) => return None,
                }
            }
            Some(Unit::LineBreak) => return None,
            Some(_) if is_last || text.is_empty() => return None,
            Some(_) => End::Wrap,
            None if is_last => End::Message,
            None => return None,
        };

        if let Some((_, col)) = cursor_here {
            // The cursor's column counts the cells before it in Logical
            // order.
            let mut cells = 0;
            let before = logical
                .iter()
                .take_while(|unit| {
                    cells += drawn.units[**unit].len();
                    cells <= col
                })
                .count();
            reading.cursor = Some((start + before).min(after_spaces));
        }
        reading.rows.push(RowRead {
            start,
            text: order.len(),
            logical,
            end,
        });
    }
    if at != units.len() {
        return None;
    }

    // The app wraps only a word that does not fit on the row, and cuts
    // only a word that fills the row. Two rows that this message would
    // have been drawn on as one are another message: the same characters
    // with a line break between them.
    let cells = |row: usize, places: Range<usize>| -> usize {
        reading.rows[row].logical[places]
            .iter()
            .map(|unit| screen[row].units[*unit].len())
            .sum()
    };
    for (row, read) in reading.rows.iter().enumerate() {
        let (End::Wrap, Some(below)) = (read.end, reading.rows.get(row + 1)) else {
            continue;
        };
        let text = cells(row, 0..read.text);
        let spaces = below.start - (read.start + read.text);
        let next_word = units[below.start..below.start + below.text]
            .iter()
            .take_while(|unit| matches!(unit, Unit::Character(..)) && !unit.is_space())
            .count();
        let would_have_wrapped = match spaces {
            0 => text >= wrap_width,
            _ => text + spaces + cells(row + 1, 0..next_word) > wrap_width,
        };
        if !would_have_wrapped {
            return None;
        }
    }
    Some(reading)
}

/// Reads the message from the screen alone. `None` when some row has more
/// than one possible order.
fn read_from_screen(screen: &[Drawn]) -> Option<Vec<Unit>> {
    let mut units = Vec::new();
    for (row, drawn) in screen.iter().enumerate() {
        let end = (0..drawn.units.len())
            .rposition(|unit| !drawn.is_blank(unit))
            .map_or(0, |last| last + 1);
        let characters: Vec<char> = (0..end)
            .map(|unit| match drawn.is_blank(unit) {
                true => ' ',
                false => drawn.cell(unit).character,
            })
            .collect();
        for unit in undo::only_order(&characters)? {
            let cell = drawn.cell(unit);
            units.push(Unit::Character(characters[unit], cell.marks.clone()));
        }
        if row + 1 < screen.len() {
            units.push(Unit::RowEnd);
        }
    }
    Some(units)
}

/// The direction the first letter of `characters` gives, if there is one.
fn first_letter(characters: &[char]) -> Option<Direction> {
    characters
        .iter()
        .find_map(|character| match layout::bearing(*character) {
            Bearing::Left => Some(Direction::LeftToRight),
            Bearing::Right => Some(Direction::RightToLeft),
            _ => None,
        })
}

/// The Paragraph direction of each row. A line the user broke is a
/// paragraph of its own, and the rows the app wrapped it onto share its
/// direction. Where the screen does not say which of the two ended a row,
/// the row below starts a paragraph if its first word would have fitted on
/// the row: the app wraps only a word that does not fit.
fn directions(
    units: &[Unit],
    reading: &Reading,
    screen: &[Drawn],
    wrap_width: usize,
) -> Vec<Direction> {
    let cells = |row: usize, logical: Range<usize>| -> usize {
        reading.rows[row].logical[logical]
            .iter()
            .map(|unit| screen[row].units[*unit].len())
            .sum()
    };
    let first_word = |row: usize| -> usize {
        let read = &reading.rows[row];
        let word = units[read.start..read.start + read.text]
            .iter()
            .take_while(|unit| !unit.is_space())
            .count();
        cells(row, 0..word)
    };
    let starts_a_paragraph = |row: usize| -> bool {
        match row
            .checked_sub(1)
            .map(|above| (above, reading.rows[above].end))
        {
            None | Some((_, End::LineBreak | End::Message)) => true,
            Some((_, End::Wrap)) => false,
            Some((above, End::Unknown)) => {
                cells(above, 0..reading.rows[above].text) + 1 + first_word(row) <= wrap_width
            }
        }
    };
    let mut directions: Vec<Direction> = Vec::with_capacity(reading.rows.len());
    let mut row = 0;
    while row < reading.rows.len() {
        let next = (row + 1..reading.rows.len())
            .find(|row| starts_a_paragraph(*row))
            .unwrap_or(reading.rows.len());
        let end = reading
            .rows
            .get(next)
            .map_or(units.len(), |read| read.start);
        let characters: Vec<char> = units[reading.rows[row].start..end]
            .iter()
            .filter_map(|unit| match unit {
                Unit::Character(character, _) => Some(*character),
                _ => None,
            })
            .collect();
        let direction = first_letter(&characters).unwrap_or(Direction::LeftToRight);
        directions.extend(std::iter::repeat_n(direction, next - row));
        row = next;
    }
    directions
}

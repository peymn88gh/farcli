//! Editing keys: which way the cursor of a text field goes for an arrow
//! key, and which keys farcli does not forward as they were pressed.
//!
//! A Target app moves its cursor through the message in Logical order: its
//! Left key goes one character towards the start of the message, wherever
//! that character is on screen. In a text field the user expects what Word
//! and browsers do: Left moves the cursor one place to the left on screen,
//! Right one place to the right.
//!
//! This is the one place that says which keys are not forwarded as pressed
//! (`Arrow::read`): the Left and Right arrows, alone or with Shift, Ctrl or
//! Alt held. Every other key reaches the Target app as it came.
//!
//! Nothing here knows about any one Target app.

use crate::layout::{self, Direction};
use crate::screen::Cell;
use crate::terminal_input::{Input, Key};

/// One row of a message shown as a text field.
pub(super) struct FieldRow {
    /// One character for each place the cursor can be on the row, in
    /// Logical order: the character the cursor is on at that place, and a
    /// space for a place after the row's text.
    pub characters: Vec<char>,
    /// The direction the row is laid out in.
    pub direction: Direction,
}

impl FieldRow {
    /// The row's places in the order their cells sit on screen, from the
    /// left. Layout says, which also lays the row out for the Terminal: so
    /// the cursor goes by what the user sees, whatever rules Layout
    /// follows.
    fn on_screen(&self) -> Vec<usize> {
        let cells: Vec<Cell> = self
            .characters
            .iter()
            .map(|character| Cell {
                character: *character,
                ..Cell::blank()
            })
            .collect();
        let line = layout::lay_out_line(&cells, self.direction, cells.len());
        (0..cells.len()).map(|col| line.logical_col(col)).collect()
    }
}

/// A side of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Left,
    Right,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

/// The place the cursor goes to from the place `from` when it moves one
/// place towards `side` on screen. A place is a row and one of the row's
/// `characters`. `None` when there is no place on that side: at the edge of
/// the first row or of the last.
///
/// On a row the cursor goes to the place shown beside it, whatever the
/// directions of the runs it passes. Past the end of a row it goes on to
/// the row that follows in reading order: for a row laid out right to left
/// that is the row below at its left end and the row above at its right
/// end. It enters that row at the end nearest in reading order, which is
/// the end the row starts from when it comes from above and the end the
/// row finishes at when it comes from below.
pub(super) fn beside(
    rows: &[FieldRow],
    from: (usize, usize),
    side: Side,
) -> Option<(usize, usize)> {
    let (row, place) = from;
    let here = rows.get(row)?;
    let order = here.on_screen();
    let on_screen = order.iter().position(|logical| *logical == place)?;
    let beside = match side {
        Side::Left => on_screen.checked_sub(1),
        Side::Right => Some(on_screen + 1).filter(|next| *next < order.len()),
    };
    if let Some(beside) = beside {
        return Some((row, order[beside]));
    }
    let onwards = leads_onwards(here.direction, side);
    let next = match onwards {
        true => row + 1,
        false => row.checked_sub(1)?,
    };
    let entered = rows.get(next)?;
    let order = entered.on_screen();
    let at_its_right_end = (entered.direction == Direction::RightToLeft) == onwards;
    let place = match at_its_right_end {
        true => order.last(),
        false => order.first(),
    };
    Some((next, *place?))
}

/// True when going towards `side` on a row laid out in `direction` is going
/// on in reading order, towards the end of the message.
pub(super) fn leads_onwards(direction: Direction, side: Side) -> bool {
    (direction == Direction::RightToLeft) == (side == Side::Left)
}

/// How far an arrow key takes the Target app's cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reach {
    /// One character: the arrow alone, or with Shift held.
    Character,
    /// To the start or the end of a word: the arrow with Ctrl or Alt held.
    Word,
}

/// The Left or Right arrow key, going down or coming up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Arrow {
    pub side: Side,
    pub reach: Reach,
    pub down: bool,
    /// How many times the key was pressed.
    pub repeats: usize,
}

/// Windows' numbers for the two arrow keys, and the keyboard's own.
const LEFT_KEY: (u16, u16) = (37, 75);
const RIGHT_KEY: (u16, u16) = (39, 77);

/// Which of Alt and Ctrl are held, as Windows numbers them.
const ALT_OR_CTRL: u32 = 0x1 | 0x2 | 0x4 | 0x8;

impl Arrow {
    /// Reads `input` as an arrow key, in the Windows form or as Terminals
    /// send it that do not send keys in that form: `ESC [ D` and `ESC [ C`,
    /// with `1 ;` and a number before the letter when Shift, Alt or Ctrl is
    /// held.
    pub fn read(input: &Input) -> Option<Arrow> {
        match input {
            Input::Key(key) => {
                let side = match key.key {
                    number if number == LEFT_KEY.0 => Side::Left,
                    number if number == RIGHT_KEY.0 => Side::Right,
                    _ => return None,
                };
                let reach = match key.modifiers & ALT_OR_CTRL {
                    0 => Reach::Character,
                    _ => Reach::Word,
                };
                Some(Arrow {
                    side,
                    reach,
                    down: key.down,
                    repeats: usize::from(key.repeats.clamp(1, 64)),
                })
            }
            Input::Other(code) => {
                let (held, side) = match code.as_slice() {
                    b"\x1b[D" => (b'1', Side::Left),
                    b"\x1b[C" => (b'1', Side::Right),
                    [0x1b, b'[', b'1', b';', held @ b'2'..=b'8', b'D'] => (*held, Side::Left),
                    [0x1b, b'[', b'1', b';', held @ b'2'..=b'8', b'C'] => (*held, Side::Right),
                    _ => return None,
                };
                // One more than: 1 for Shift, 2 for Alt, 4 for Ctrl.
                let reach = match held {
                    b'1' | b'2' => Reach::Character,
                    _ => Reach::Word,
                };
                Some(Arrow {
                    side,
                    reach,
                    down: true,
                    repeats: 1,
                })
            }
            _ => None,
        }
    }

    /// One press of the arrow key that points to `side`, in the form
    /// `pressed` came in and with the same keys held: going down and, for a
    /// key in the Windows form, coming up.
    pub fn press(pressed: &Input, side: Side) -> Vec<Input> {
        match pressed {
            Input::Key(key) => {
                let (number, scan_code) = match side {
                    Side::Left => LEFT_KEY,
                    Side::Right => RIGHT_KEY,
                };
                [true, false]
                    .map(|down| {
                        Input::Key(Key {
                            key: number,
                            scan_code,
                            down,
                            repeats: 1,
                            ..*key
                        })
                    })
                    .to_vec()
            }
            Input::Other(code) => {
                let mut code = code.clone();
                if let Some(letter) = code.last_mut() {
                    *letter = match side {
                        Side::Left => b'D',
                        Side::Right => b'C',
                    };
                }
                vec![Input::Other(code)]
            }
            other => vec![other.clone()],
        }
    }
}

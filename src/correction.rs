//! Correction: works out what farcli shows in place of what the Target app
//! drew.
//!
//! For now there is one rule, for every program: a row that holds Farsi is a
//! paragraph of one line, as wide as the Terminal.

use crate::layout::{self, Line};
use crate::painter::Frame;
use crate::screen::{Cell, ScreenChange};

/// Turns the frame the Target app drew into the frame farcli shows.
pub(crate) fn correct(frame: &mut Frame) {
    for change in &mut frame.changes {
        if let ScreenChange::ScrolledOff(row) = change {
            if let Some(line) = laid_out(row) {
                *row = line.cells;
            }
        }
    }
    for (index, row) in frame.rows.iter_mut().enumerate() {
        let Some(line) = laid_out(row) else {
            continue;
        };
        if usize::from(frame.cursor.row) == index {
            // The app's cursor is at a Logical position on this row.
            frame.cursor.col = line.screen_col(usize::from(frame.cursor.col)) as u16;
        }
        *row = line.cells;
    }
}

/// The row laid out as a one-line paragraph, or `None` when the row is to be
/// shown as the app drew it.
fn laid_out(row: &[Cell]) -> Option<Line> {
    let text_end = row
        .iter()
        .rposition(|cell| !cell.is_blank() && !cell.is_left_alone())
        .map_or(0, |last| last + 1);
    let text = &row[..text_end];
    if !layout::holds_right_to_left(text) {
        return None;
    }
    // Text the app did not draw is not farcli's to move, and the row cannot
    // be laid out around it.
    if text.iter().any(Cell::is_left_alone) {
        return None;
    }
    let direction = layout::paragraph_direction(text);
    Some(layout::lay_out_line(text, direction, row.len()))
}

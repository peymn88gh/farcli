//! Correction: works out what farcli shows in place of what the Target app
//! drew.
//!
//! For now there is one rule, for a program farcli knows nothing about: a
//! row that holds Farsi is a paragraph of one line, as wide as the Terminal.

use crate::app_profile::AppProfile;
use crate::layout::{self, Line};
use crate::painter::Frame;
use crate::screen::{Cell, ScreenChange};

/// Turns the frame the Target app drew into the frame farcli shows.
pub(crate) fn correct(frame: &mut Frame, profile: &AppProfile) {
    if !profile.every_row_is_a_paragraph() {
        return;
    }
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
    // A row whose start the app did not draw may show something there that
    // is not the app's, and so not farcli's to move.
    if text.first().is_some_and(Cell::is_left_alone) {
        return None;
    }
    // Cells the app moved over without drawing, between cells it did draw,
    // count as empty.
    let text: Vec<Cell> = text
        .iter()
        .map(|cell| match cell.is_left_alone() {
            true => Cell::blank(),
            false => cell.clone(),
        })
        .collect();
    let direction = layout::paragraph_direction(&text);
    Some(layout::lay_out_line(&text, direction, row.len()))
}

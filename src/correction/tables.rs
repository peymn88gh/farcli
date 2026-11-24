//! Tables in Agent output: the text inside each cell is laid out as a
//! paragraph of its own, in the cell's own width, and nothing else moves.
//! The columns stay in the order the Target app wrote them, first column
//! at the left, and the borders stay where they are.
//!
//! The two Target apps draw a table differently:
//!
//! - Codex draws no upright bars. A rule under the header and between the
//!   rows shows where each column is, and the text of a cell starts one
//!   column in from the left end of its rule. Rows are in Logical order.
//! - Claude Code draws a box round every cell. It reorders each row as one
//!   piece of text, bars and all, so a row that begins with a Farsi cell
//!   is drawn with its columns in the opposite order to the borders above
//!   and below it, and its bars do not line up with them. The Undo step
//!   turns the whole row back; then the bars stand under the borders
//!   again, which is also the check that the row was read rightly.
//!
//! A row that does not fit what is known of its table is left as drawn.

use std::ops::Range;

use crate::layout::{self, Direction};
use crate::regions::Region;
use crate::screen::{Cell, Row};
use crate::undo;

use super::{drawn_cells, drawn_row, key, Correction, Seen};

/// The characters Codex draws a rule with.
const RULES: [char; 3] = ['─', '━', '═'];

/// The upright bar between two cells of a table with borders.
const BAR: char = '│';

impl Correction {
    /// Lays out the cells of one table in place.
    pub(super) fn table(&mut self, rows: &mut [Row], region: &Region) {
        if self.profile.reorders_farsi_itself() {
            self.table_with_borders(rows, region);
        } else {
            self.table_with_rules(rows, region);
        }
    }

    /// A table as Codex draws it. The columns are those of its first rule.
    /// A row with no rule in sight, at the top of the screen, has the
    /// columns it had when it was last read with one.
    fn table_with_rules(&mut self, rows: &mut [Row], region: &Region) {
        let area = region.cols.clone();
        let from_a_rule = region
            .rows
            .clone()
            .find_map(|row| rule_columns(&rows[row][area.clone()]));
        // The direction of each column's cell in the table row being read:
        // a cell wrapped onto several rows keeps the direction of its
        // first.
        let mut directions: Vec<Option<Direction>> = Vec::new();
        for row in region.rows.clone() {
            let drawn = key(&drawn_row(&rows[row][area.clone()], false));
            let cells = &mut rows[row][area.clone()];
            if rule_columns(cells).is_some() {
                directions.clear();
                continue;
            }
            let columns = match (&from_a_rule, self.seen.get(&drawn)) {
                (Some(columns), _) => columns.clone(),
                (None, Some(Seen::InTable(columns))) => columns.clone(),
                (None, _) => continue,
            };
            if !sits_in_columns(cells, &columns) {
                continue;
            }
            directions.resize(columns.len(), None);
            for (column, direction) in columns.iter().zip(&mut directions) {
                lay_out_cell(&mut cells[column.clone()], direction);
            }
            self.remember(drawn, Seen::InTable(columns));
        }
    }

    /// A table as Claude Code draws it. The columns are those of its
    /// borders, and each row is turned back into Logical order first.
    fn table_with_borders(&mut self, rows: &mut [Row], region: &Region) {
        let area = region.cols.clone();
        let Some(bars) = region
            .rows
            .clone()
            .find_map(|row| border_bars(&rows[row][area.clone()]))
        else {
            return;
        };
        let mut directions: Vec<Option<Direction>> = Vec::new();
        for row in region.rows.clone() {
            let cells = &mut rows[row][area.clone()];
            if border_bars(cells).is_some() {
                directions.clear();
                continue;
            }
            let drawn = drawn_row(cells, false);
            if !layout::holds_right_to_left(&drawn.cells) {
                continue;
            }
            let Some(written) = row_as_written(&drawn, &bars) else {
                continue;
            };
            cells[..written.len()].clone_from_slice(&written);
            directions.resize(bars.len() - 1, None);
            for (pair, direction) in bars.windows(2).zip(&mut directions) {
                lay_out_cell(&mut cells[pair[0] + 1..pair[1]], direction);
            }
        }
    }
}

/// Lays out the text of one cell in place, in the cell's own direction.
/// `direction` is that of the same cell on the row above when the cell is
/// wrapped onto several rows, and is set by the first row that holds a
/// letter.
fn lay_out_cell(cell: &mut [Cell], direction: &mut Option<Direction>) {
    let written = drawn_cells(cell);
    if direction.is_none() && layout::holds_a_letter(&written) {
        *direction = Some(layout::paragraph_direction(&written));
    }
    let direction = direction.unwrap_or_else(|| layout::paragraph_direction(&written));
    // A left-to-right cell with no Farsi in it is already what Layout
    // would make of it.
    if direction == Direction::LeftToRight && !layout::holds_right_to_left(&written) {
        return;
    }
    let line = layout::lay_out_line(&written, direction, cell.len());
    cell.clone_from_slice(&line.cells);
}

fn is_empty(cell: &Cell) -> bool {
    cell.is_left_alone() || cell.character == ' '
}

/// The columns a rule of Codex's shows, as places in the row: each run of
/// rule characters is one column. `None` for a row that is not a rule.
fn rule_columns(row: &[Cell]) -> Option<Vec<Range<usize>>> {
    let mut columns: Vec<Range<usize>> = Vec::new();
    for (at, cell) in row.iter().enumerate() {
        if is_empty(cell) {
            continue;
        }
        if !RULES.contains(&cell.character) {
            return None;
        }
        match columns.last_mut() {
            Some(column) if column.end == at => column.end = at + 1,
            _ => columns.push(at..at + 1),
        }
    }
    (!columns.is_empty()).then_some(columns)
}

/// True when every character of the row is inside a column and none is in
/// a column's first cell, which Codex leaves empty before a cell's text.
fn sits_in_columns(row: &[Cell], columns: &[Range<usize>]) -> bool {
    row.iter().enumerate().all(|(at, cell)| {
        is_empty(cell)
            || columns
                .iter()
                .any(|column| column.start < at && at < column.end)
    })
}

/// Where the upright strokes of a border row of Claude Code's are, as
/// places in the row: its corners and the joints between its columns.
/// `None` for a row that is not a border.
fn border_bars(row: &[Cell]) -> Option<Vec<usize>> {
    let is_line_drawing = |character: char| ('\u{2500}'..='\u{257f}').contains(&character);
    let mut bars = Vec::new();
    let mut dashes = 0;
    for (at, cell) in row.iter().enumerate() {
        match cell.character {
            _ if is_empty(cell) => {}
            '─' => dashes += 1,
            character if is_line_drawing(character) && character != BAR => bars.push(at),
            _ => return None,
        }
    }
    (dashes > 0 && bars.len() >= 2).then_some(bars)
}

/// A row of a table with borders in Logical order: the reading of what
/// the app drew in which the bars stand under the table's borders. `None`
/// when no reading does.
///
/// A row with an English cell at its left end and a Farsi one at its right
/// end can be read both ways, and in a table whose columns mirror each
/// other in width the bars fit both times. Then the reading is taken in
/// which more cells have their text one column in from the bar on their
/// left, as the app draws the cells under a header.
fn row_as_written(drawn: &undo::DrawnRow, bars: &[usize]) -> Option<Vec<Cell>> {
    let fits = |written: &Vec<Cell>| {
        let found: Vec<usize> = (0..written.len())
            .filter(|at| written[*at].character == BAR)
            .collect();
        found == bars
    };
    let starts_one_in = |written: &Vec<Cell>| {
        bars.iter()
            .filter(|bar| {
                written.get(*bar + 1).is_some_and(is_empty)
                    && written.get(*bar + 2).is_some_and(|cell| !is_empty(cell))
            })
            .count()
    };
    let mut readings: Vec<Vec<Cell>> = undo::readings(drawn).into_iter().filter(fits).collect();
    // A stable sort: of two readings that fit equally well, the likelier
    // one stays first.
    readings.sort_by_key(|written| std::cmp::Reverse(starts_one_in(written)));
    readings.into_iter().next()
}

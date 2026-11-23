//! Correction: works out what farcli shows in place of what the Target app
//! drew.
//!
//! For a program farcli knows nothing about, a row that holds Farsi is a
//! paragraph of one line, as wide as the Terminal. For a Target app, the
//! Region finder says where the paragraphs of Agent output are, and only
//! those are laid out again: the rest of the screen is shown as drawn.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::app_profile::{AppProfile, Screen};
use crate::layout::{self, Direction, Line};
use crate::painter::Frame;
use crate::regions::claude_code::Finder;
use crate::regions::{Kind, ParagraphStart, Region};
use crate::screen::{Cell, EraseScreen, Row, ScreenChange};
use crate::undo::{self, DrawnRow, Start};

/// Turns each frame a Target app drew into the frame farcli shows. It
/// follows one session from its start, because what a row is can depend on
/// rows that have since scrolled off the top.
pub(crate) struct Correction {
    profile: AppProfile,
    /// Finds the regions of the main screen, where rows scroll off the top
    /// and the finder carries on from them.
    finder: Finder,
    on_alternate_screen: bool,
    /// The direction of the paragraph that the top of the main screen cuts
    /// through, when there is one.
    direction_above: Option<Direction>,
    /// On the alternate screen the app draws the conversation again at
    /// whatever place the user scrolls to. This holds what each row of a
    /// reply was found to be while the rows before it were in sight, so
    /// that it reads the same when they are scrolled out of sight.
    seen: HashMap<u64, Seen>,
}

/// What a row of a reply was found to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seen {
    /// A row of a paragraph with this Paragraph direction.
    InParagraph(Direction),
    /// A row of something that is shown as drawn.
    NotInParagraph,
}

/// More rows than a long session shows. Past this the rows seen are
/// forgotten, and are learnt again as they are shown.
const MOST_ROWS_SEEN: usize = 20_000;

impl Correction {
    pub fn new(profile: AppProfile) -> Correction {
        Correction {
            profile,
            finder: Finder::new(),
            on_alternate_screen: false,
            direction_above: None,
            seen: HashMap::new(),
        }
    }

    /// Turns the frame the Target app drew into the frame farcli shows.
    pub fn correct(&mut self, frame: &mut Frame) {
        match self.profile.screen() {
            Screen::EveryRowAParagraph => every_row_a_paragraph(frame),
            Screen::ClaudeCode => self.agent_output(frame),
            Screen::NotKnown => {}
        }
    }

    /// Lays out the paragraphs of Agent output: on the rows that scrolled
    /// off the top since the last frame, which the Terminal keeps for
    /// scrolling back, and on the rows on screen.
    fn agent_output(&mut self, frame: &mut Frame) {
        // The rows that scrolled off since the screen was last cleared or
        // switched, as places in `changes`. Read from the top down, they
        // and then the rows on screen are one stretch of the conversation.
        let mut scrolled_off: Vec<usize> = Vec::new();
        for (at, change) in frame.changes.iter().enumerate() {
            match change {
                ScreenChange::ScrolledOff(_) => scrolled_off.push(at),
                ScreenChange::Erased {
                    part: EraseScreen::All,
                    ..
                } => {
                    scrolled_off.clear();
                    self.finder.screen_cleared();
                    self.direction_above = None;
                }
                ScreenChange::Erased { .. } => {}
                ScreenChange::AlternateScreen(alternate) => {
                    scrolled_off.clear();
                    self.on_alternate_screen = *alternate;
                }
            }
        }

        let mut rows: Vec<Row> = Vec::with_capacity(scrolled_off.len() + frame.rows.len());
        for at in &scrolled_off {
            if let ScreenChange::ScrolledOff(row) = &mut frame.changes[*at] {
                rows.push(std::mem::take(row));
            }
        }
        rows.append(&mut frame.rows);

        self.paragraphs(&mut rows, scrolled_off.len());

        frame.rows = rows.split_off(scrolled_off.len());
        for (at, corrected) in scrolled_off.into_iter().zip(rows) {
            frame.changes[at] = ScreenChange::ScrolledOff(corrected);
        }
    }

    /// Lays out every paragraph of Agent output in `rows`, of which the
    /// first `leaving` are scrolling off the top of the screen.
    fn paragraphs(&mut self, rows: &mut [Row], leaving: usize) {
        let regions = if self.on_alternate_screen {
            Finder::knowing_nothing_above().find(rows, 0)
        } else {
            self.finder.find(rows, leaving)
        };
        let mut cut_by_the_top = None;
        for region in &regions {
            let Kind::Paragraph(start) = region.kind else {
                if self.on_alternate_screen && region.kind != Kind::InputBox {
                    for row in region.rows.clone() {
                        let row = drawn_row(&rows[row][region.cols.clone()], false);
                        self.remember(&row, Seen::NotInParagraph);
                    }
                }
                continue;
            };
            let Some(direction) = self.paragraph(rows, region, start) else {
                continue;
            };
            if region.rows.start < leaving && leaving < region.rows.end {
                cut_by_the_top = Some(direction);
            }
        }
        if leaving > 0 {
            self.direction_above = cut_by_the_top;
        }
    }

    fn remember(&mut self, row: &DrawnRow, seen: Seen) {
        if self.seen.len() > MOST_ROWS_SEEN {
            self.seen.clear();
        }
        self.seen.insert(key(row), seen);
    }

    /// Lays out one paragraph in place and returns its direction. Returns
    /// `None`, with nothing changed, for rows that were found earlier not
    /// to be a paragraph.
    fn paragraph(
        &mut self,
        rows: &mut [Row],
        region: &Region,
        start: ParagraphStart,
    ) -> Option<Direction> {
        let width = region.cols.len();
        let last = region.rows.end - 1;
        let drawn: Vec<DrawnRow> = region
            .rows
            .clone()
            .map(|row| drawn_row(&rows[row][region.cols.clone()], row < last))
            .collect();
        let start = match start {
            ParagraphStart::Here => Start::Here,
            ParagraphStart::Above if self.on_alternate_screen => {
                match drawn.first().and_then(|row| self.seen.get(&key(row))) {
                    Some(Seen::NotInParagraph) => return None,
                    Some(Seen::InParagraph(direction)) => Start::Above(Some(*direction)),
                    None => Start::Above(None),
                }
            }
            ParagraphStart::Above => Start::Above(self.direction_above),
        };
        // The only screen with regions so far is Claude Code's, and Claude
        // Code reorders Farsi itself. The rows of an app that draws Logical
        // order are the paragraph's lines as they are, with nothing to
        // undo: this is where such an app parts ways.
        debug_assert!(self.profile.reorders_farsi_itself());
        let paragraph = undo::paragraph(&drawn, width, start);

        if self.on_alternate_screen && start == Start::Here {
            for row in &drawn {
                self.remember(row, Seen::InParagraph(paragraph.direction));
            }
        }
        for (row, logical) in region.rows.clone().zip(&paragraph.rows) {
            let Some(logical) = logical else {
                continue;
            };
            // A left-to-right row with no Farsi in it is already what
            // Layout would make of it.
            if paragraph.direction == Direction::LeftToRight
                && !layout::holds_right_to_left(logical)
            {
                continue;
            }
            let line = layout::lay_out_line(logical, paragraph.direction, width);
            rows[row][region.cols.clone()].clone_from_slice(&line.cells);
        }
        Some(paragraph.direction)
    }
}

/// One row of a paragraph as the app drew it in its area: the cells up to
/// the row's last character.
fn drawn_row(area: &[Cell], wrapped: bool) -> DrawnRow {
    let end = area
        .iter()
        .rposition(|cell| !cell.is_blank() && !cell.is_left_alone())
        .map_or(0, |last| last + 1);
    DrawnRow {
        cells: drawn_cells(&area[..end]),
        wrapped,
    }
}

/// Cells the app moved over without drawing, between cells it did draw,
/// count as empty.
fn drawn_cells(cells: &[Cell]) -> Vec<Cell> {
    cells
        .iter()
        .map(|cell| match cell.is_left_alone() {
            true => Cell::blank(),
            false => cell.clone(),
        })
        .collect()
}

/// What a drawn row is remembered by.
fn key(row: &DrawnRow) -> u64 {
    let mut hasher = DefaultHasher::new();
    for cell in &row.cells {
        cell.character.hash(&mut hasher);
        cell.marks.hash(&mut hasher);
    }
    hasher.finish()
}

/// For a program farcli knows nothing about: each row that holds Farsi is
/// laid out as a paragraph of one line, as wide as the Terminal.
fn every_row_a_paragraph(frame: &mut Frame) {
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
    let text = drawn_cells(text);
    let direction = layout::paragraph_direction(&text);
    Some(layout::lay_out_line(&text, direction, row.len()))
}

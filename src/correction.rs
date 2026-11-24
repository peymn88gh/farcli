//! Correction: works out what farcli shows in place of what the Target app
//! drew.
//!
//! For a program farcli knows nothing about, a row that holds Farsi is a
//! paragraph of one line, as wide as the Terminal. For a Target app, the
//! Region finder says where the paragraphs, the list items and the tables
//! of Agent output are, and only those are laid out again: the rest of the
//! screen is shown as drawn.
//!
//! A list item is its marker and a paragraph: the item's rows are laid
//! out whole, marker first, so in a Farsi item the marker is on the right
//! and the rows the text wraps onto end under the text. The cells of a
//! table are in `tables.rs`.

mod tables;

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use crate::app_profile::{AppProfile, Screen};
use crate::input_box::{InputBox, Place};
use crate::layout::{self, Direction, Line};
use crate::painter::Frame;
use crate::regions::claude_code;
use crate::regions::{Finder, ItemStart, Kind, ParagraphStart, Region};
use crate::screen::{Cell, Color, EraseScreen, Position, Row, ScreenChange};
use crate::terminal_input::Input;
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
    /// The app has drawn something in colour since the session started.
    /// An app run with colours switched off never does, and then its code
    /// blocks cannot be told from lines of prose.
    draws_in_colour: bool,
    /// The direction of the paragraph that the top of the main screen cuts
    /// through, when there is one.
    direction_above: Option<Direction>,
    /// On the alternate screen the app draws the conversation again at
    /// whatever place the user scrolls to. This holds what each row of a
    /// reply was found to be while the rows before it were in sight, so
    /// that it reads the same when they are scrolled out of sight.
    seen: HashMap<u64, Seen>,
    /// The message the user is typing in the app's Input box.
    input_box: InputBox,
}

/// What a row of a reply was found to be.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seen {
    /// A row of a paragraph with this Paragraph direction.
    InParagraph(Direction),
    /// A row of a list item that runs in this direction, whose text starts
    /// `text` columns into the reply's area.
    InListItem { direction: Direction, text: usize },
    /// A row of a table whose columns are these stretches of the reply's
    /// area.
    InTable(Vec<Range<usize>>),
    /// A row of something that is shown as drawn.
    NotInParagraph,
}

/// More rows than a long session shows. Past this the rows seen are
/// forgotten, and are learnt again as they are shown.
const MOST_ROWS_SEEN: usize = 20_000;

impl Correction {
    pub fn new(profile: AppProfile) -> Correction {
        Correction {
            finder: Finder::for_screen(profile.screen()),
            profile,
            on_alternate_screen: false,
            draws_in_colour: false,
            direction_above: None,
            seen: HashMap::new(),
            input_box: InputBox::new(),
        }
    }

    /// The Terminal sent `input`, which is on its way to the Target app:
    /// what the user typed is one of the two witnesses of what the Input
    /// box holds.
    pub fn user_typed(&mut self, input: &Input) {
        if self.profile.screen() == Screen::ClaudeCode {
            self.input_box.user_typed(input);
        }
    }

    /// Turns the frame the Target app drew into the frame farcli shows.
    pub fn correct(&mut self, frame: &mut Frame) {
        match self.profile.screen() {
            Screen::EveryRowAParagraph => every_row_a_paragraph(frame),
            Screen::ClaudeCode | Screen::Codex => self.agent_output(frame),
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

        let regions = self.paragraphs(&mut rows, scrolled_off.len());
        let cursor = frame.cursor_visible.then_some(&mut frame.cursor);
        self.input_box(&mut rows, &regions, scrolled_off.len(), cursor);

        frame.rows = rows.split_off(scrolled_off.len());
        for (at, corrected) in scrolled_off.into_iter().zip(rows) {
            frame.changes[at] = ScreenChange::ScrolledOff(corrected);
        }
    }

    /// Lays out the message being typed in the Input box as a text field,
    /// and puts the cursor where the next character will appear. `rows`
    /// are the rows scrolling off the top, `leaving` of them, and then the
    /// rows on screen, where `cursor` is when the app shows it.
    fn input_box(
        &mut self,
        rows: &mut [Row],
        regions: &[Region],
        leaving: usize,
        cursor: Option<&mut Position>,
    ) {
        // Only Claude Code's Input box is known so far.
        if self.profile.screen() != Screen::ClaudeCode {
            return;
        }
        // The box the user types in is the last thing the app draws that
        // is wholly on screen.
        let Some(message) = regions
            .iter()
            .rev()
            .filter(|region| region.rows.start >= leaving)
            .find_map(|region| claude_code::typed_message(rows, region))
        else {
            return;
        };
        let cursor_in_message = cursor.as_ref().and_then(|cursor| {
            let row = leaving + usize::from(cursor.row);
            let col = usize::from(cursor.col);
            (message.rows.contains(&row) && message.cols.contains(&col))
                .then(|| (row - message.rows.start, col - message.cols.start))
        });
        let col = self.input_box.correct(Place {
            rows: &mut rows[message.rows.clone()],
            cols: message.cols.clone(),
            wrap_width: message.wrap_width,
            cursor: cursor_in_message,
            is_empty: message.is_empty,
        });
        if let (Some(col), Some(cursor)) = (col, cursor) {
            cursor.col = (message.cols.start + col) as u16;
        }
    }

    /// Lays out every paragraph of Agent output in `rows`, of which the
    /// first `leaving` are scrolling off the top of the screen, and returns
    /// the regions found in them.
    fn paragraphs(&mut self, rows: &mut [Row], leaving: usize) -> Vec<Region> {
        self.draws_in_colour = self.draws_in_colour
            || rows.iter().flatten().any(|cell| {
                !cell.is_left_alone()
                    && (cell.style.foreground != Color::Default
                        || cell.style.background != Color::Default)
            });
        let regions = if self.on_alternate_screen {
            self.finder
                .knowing_nothing_above()
                .find(rows, 0, self.draws_in_colour)
        } else {
            self.finder.find(rows, leaving, self.draws_in_colour)
        };
        let mut cut_by_the_top = None;
        // The list item before the one being read, when the two stand
        // under one another with no row between: the row after its last,
        // and its direction.
        let mut item_before: Option<(usize, Direction)> = None;
        for region in &regions {
            match region.kind {
                Kind::Paragraph(start) => {
                    let Some(direction) = self.paragraph(rows, region, start) else {
                        continue;
                    };
                    if region.rows.start < leaving && leaving < region.rows.end {
                        cut_by_the_top = Some(direction);
                    }
                }
                Kind::ListItem(start) => {
                    let list = item_before
                        .filter(|(end, _)| *end == region.rows.start)
                        .map(|(_, direction)| direction);
                    if let Some(direction) = self.list_item(rows, region, start, list) {
                        item_before = Some((region.rows.end, direction));
                    }
                }
                Kind::Table => self.table(rows, region),
                Kind::InputBox => {}
                // Rows whose start is out of sight, which the finder could
                // not name, read as they did when it was in sight.
                Kind::Code | Kind::Unsure if region.cut_above => {
                    self.as_seen_before(rows, region);
                }
                _ => self.left_as_drawn(rows, region),
            }
        }
        if leaving > 0 {
            self.direction_above = cut_by_the_top;
        }
        regions
    }

    fn remember(&mut self, row: u64, seen: Seen) {
        if self.seen.len() > MOST_ROWS_SEEN {
            self.seen.clear();
        }
        self.seen.insert(row, seen);
    }

    /// Notes that the rows of a region are shown as drawn, so that they
    /// are not taken for a paragraph when the rows before them are out of
    /// sight.
    fn left_as_drawn(&mut self, rows: &[Row], region: &Region) {
        if !self.on_alternate_screen {
            return;
        }
        for row in region.rows.clone() {
            let row = key(&drawn_row(&rows[row][region.cols.clone()], false));
            // A row once read with everything about it in sight keeps that
            // reading.
            let read_before = matches!(
                self.seen.get(&row),
                Some(Seen::InListItem { .. } | Seen::InTable(_))
            );
            if !read_before {
                self.remember(row, Seen::NotInParagraph);
            }
        }
    }

    /// Lays out rows the finder could not name because what they carry on
    /// is above the rows given, when they were read before with that in
    /// sight.
    fn as_seen_before(&mut self, rows: &mut [Row], region: &Region) {
        let first = key(&drawn_row(
            &rows[region.rows.start][region.cols.clone()],
            false,
        ));
        match self.seen.get(&first) {
            Some(Seen::InListItem { .. }) => {
                self.list_item(rows, region, ItemStart::Above, None);
            }
            Some(Seen::InTable(_)) => self.table(rows, region),
            _ => self.left_as_drawn(rows, region),
        }
    }

    /// Lays out one list item in place and returns its direction: the
    /// marker and the indentation before it, then the item's text, row by
    /// row. `list` is the direction of the item right above this one.
    /// Returns `None`, with nothing changed, for rows an item wrapped onto
    /// of which too little is known.
    fn list_item(
        &mut self,
        rows: &mut [Row],
        region: &Region,
        start: ItemStart,
        list: Option<Direction>,
    ) -> Option<Direction> {
        let area = region.cols.clone();
        let keys: Vec<u64> = region
            .rows
            .clone()
            .map(|row| key(&drawn_row(&rows[row][area.clone()], false)))
            .collect();
        // How far into the area the item's text starts, and the item's
        // direction when it is known from an earlier look at these rows.
        let (text, known) = match start {
            ItemStart::Here { text_col } => (text_col.saturating_sub(area.start), None),
            ItemStart::Above => match self.seen.get(&keys[0]) {
                Some(Seen::InListItem { direction, text }) => (*text, Some(*direction)),
                Some(_) => return None,
                // An app that reorders drew these rows as text of their
                // own, in an area that begins where the item's text does.
                // Where that is, the marker would have told.
                None if self.profile.reorders_farsi_itself() => return None,
                // Rows in Logical order need only the item's direction.
                None => (0, None),
            },
        };
        let text = text.min(area.len());
        let last = region.rows.end - 1;
        let drawn: Vec<DrawnRow> = region
            .rows
            .clone()
            .map(|row| drawn_row(&rows[row][area.start + text..area.end], row < last))
            .collect();
        let begins = match start {
            ItemStart::Here { .. } => Start::Here,
            ItemStart::Above => Start::Above(known),
        };
        let paragraph = if self.profile.reorders_farsi_itself() {
            undo::paragraph(&drawn, area.len() - text, begins)
        } else {
            in_logical_order(&drawn, begins)
        };
        let written: Vec<Cell> = paragraph.rows.iter().flatten().flatten().cloned().collect();
        let direction = match (known, layout::holds_a_letter(&written)) {
            (Some(direction), _) => direction,
            (None, true) => paragraph.direction,
            (None, false) => layout::direction_of_an_item_with_no_letter(&written, list),
        };

        for ((row, logical), key) in region.rows.clone().zip(&paragraph.rows).zip(keys) {
            self.remember(key, Seen::InListItem { direction, text });
            let Some(logical) = logical else {
                continue;
            };
            // The marker and what is before it, then the text.
            let mut line = drawn_cells(&rows[row][area.start..area.start + text]);
            line.extend(logical.iter().cloned());
            // A left-to-right row with no Farsi in it is already what
            // Layout would make of it.
            if direction == Direction::LeftToRight && !layout::holds_right_to_left(&line) {
                continue;
            }
            let line = layout::lay_out_line(&line, direction, area.len());
            rows[row][area.clone()].clone_from_slice(&line.cells);
        }
        Some(direction)
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
        let seen_before = || drawn.first().and_then(|row| self.seen.get(&key(row)));
        let start = match start {
            ParagraphStart::Here => Start::Here,
            // A line under lines that are out of sight was found to be code
            // if it was found so while they were in sight. Seen for the
            // first time, with nothing on it or beside it that tells code,
            // it is taken for the line of prose it looks like.
            ParagraphStart::HereUnderLinesOutOfSight => match seen_before() {
                Some(Seen::InParagraph(_)) | None => Start::Here,
                Some(_) => return None,
            },
            ParagraphStart::Above if self.on_alternate_screen => match seen_before() {
                Some(Seen::InParagraph(direction)) => Start::Above(Some(*direction)),
                Some(_) => return None,
                None => Start::Above(None),
            },
            ParagraphStart::Above => Start::Above(self.direction_above),
        };
        // The rows of an app that draws Logical order are the paragraph's
        // lines as they are, with nothing to undo.
        let paragraph = if self.profile.reorders_farsi_itself() {
            undo::paragraph(&drawn, width, start)
        } else {
            in_logical_order(&drawn, start)
        };

        if self.on_alternate_screen && start == Start::Here {
            for row in &drawn {
                self.remember(key(row), Seen::InParagraph(paragraph.direction));
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

/// A paragraph that the app drew in Logical order: its rows are its lines
/// as they are, and its direction is that of its first letter.
fn in_logical_order(rows: &[DrawnRow], start: Start) -> undo::Paragraph {
    let begins_in_farsi =
        |row: &&DrawnRow| layout::paragraph_direction(&row.cells) == Direction::RightToLeft;
    let direction = match start {
        Start::Above(Some(direction)) => direction,
        // With its first rows out of sight, a paragraph in which a row
        // begins with a Farsi word is far more often a Farsi paragraph
        // than an English one with Farsi in it.
        Start::Above(None) if rows.iter().any(|row| begins_in_farsi(&row)) => {
            Direction::RightToLeft
        }
        // A row that holds no letter, such as a number before the text,
        // leaves the direction to the row after it.
        _ => {
            let text: Vec<Cell> = rows.iter().flat_map(|row| row.cells.clone()).collect();
            layout::paragraph_direction(&text)
        }
    };
    undo::Paragraph {
        direction,
        rows: rows.iter().map(|row| Some(row.cells.clone())).collect(),
    }
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

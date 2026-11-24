//! Region finder: splits what a Target app drew into regions: the app's
//! interface, Agent output paragraphs, code, list items, tables and the
//! Input box.
//!
//! How a screen is read depends on the Target app, so each app has its own
//! finder. They all answer in the terms of this file. Rows that are in no
//! region are the app's interface, which is never touched, and so is a
//! region the finder could not name with confidence.

pub(crate) mod claude_code;
pub(crate) mod codex;

use std::ops::Range;

use crate::app_profile::Screen;
use crate::screen::Row;

/// The Region finder for the Target app whose screen is being read.
pub(crate) enum Finder {
    ClaudeCode(claude_code::Finder),
    Codex(codex::Finder),
}

impl Finder {
    /// A finder for a screen the app has drawn nothing on yet.
    pub fn for_screen(screen: Screen) -> Finder {
        match screen {
            Screen::Codex => Finder::Codex(codex::Finder::new()),
            // A screen with no regions never asks its finder.
            Screen::ClaudeCode | Screen::EveryRowAParagraph => {
                Finder::ClaudeCode(claude_code::Finder::new())
            }
        }
    }

    /// A finder for the same app, for rows of which nothing is known about
    /// what lies above them: a display that draws the conversation again
    /// at whatever place the user scrolled to.
    pub fn knowing_nothing_above(&self) -> Finder {
        match self {
            Finder::ClaudeCode(_) => {
                Finder::ClaudeCode(claude_code::Finder::knowing_nothing_above())
            }
            Finder::Codex(_) => Finder::Codex(codex::Finder::knowing_nothing_above()),
        }
    }

    /// The app cleared the screen: nothing of a message is above its top.
    pub fn screen_cleared(&mut self) {
        match self {
            Finder::ClaudeCode(finder) => finder.screen_cleared(),
            Finder::Codex(finder) => finder.screen_cleared(),
        }
    }

    /// The regions of `rows`, which follow one another from the top down.
    /// The first `leaving` rows are scrolling off the top of the screen.
    /// `draws_in_colour` says whether the app has drawn anything in colour
    /// in this session.
    pub fn find(&mut self, rows: &[Row], leaving: usize, draws_in_colour: bool) -> Vec<Region> {
        match self {
            Finder::ClaudeCode(finder) => finder.find(rows, leaving, draws_in_colour),
            Finder::Codex(finder) => finder.find(rows, leaving, draws_in_colour),
        }
    }
}

/// A part of the screen that holds one kind of thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Region {
    /// The rows it covers, as positions in the rows given to the finder.
    pub rows: Range<usize>,
    /// The columns of the area the app drew it in.
    pub cols: Range<usize>,
    pub kind: Kind,
    /// The region is in the first part of a reply among the rows given,
    /// and that part may have begun above them, out of sight: what told
    /// the finder what its rows are may be up there.
    pub cut_above: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// One paragraph of Agent output, row by row as the app wrapped it. A
    /// line that the writer ended with a line break is a paragraph of its
    /// own.
    Paragraph(ParagraphStart),
    /// A code block. Code is always left to right and stays as drawn.
    Code,
    /// One item of a list: its marker and, after the marker, a paragraph
    /// that carries on under its own text on the rows the app wrapped it
    /// onto.
    ListItem(ItemStart),
    /// A table with its borders, from its first row to its last.
    Table,
    /// A message the user sent, as the app shows it in the conversation.
    SentMessage,
    /// The Input box with its borders, and whatever the app draws under it.
    InputBox,
    /// Part of Agent output that the finder cannot name with confidence.
    Unsure,
}

/// Where a paragraph begins, as far as the rows given show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParagraphStart {
    /// Its first row is the region's first row.
    Here,
    /// Its first row is the region's first row, and it is one of several
    /// lines that stand under one another with no empty row between. The
    /// first of them is above the first row given, where nothing is known
    /// of them: what would have told that these lines are code may be out
    /// of sight.
    HereUnderLinesOutOfSight,
    /// It began above the first row given: its first rows have scrolled
    /// out of sight.
    Above,
}

/// Where a list item begins, as far as the rows given show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemStart {
    /// The region's first row holds the item's marker. The item's text
    /// starts in the column `text_col` of the screen, on that row and on
    /// every row the item wraps onto.
    Here { text_col: usize },
    /// The marker is above the first row given: these are rows the item
    /// wrapped onto.
    Above,
}

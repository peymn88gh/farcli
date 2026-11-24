//! Region finder: splits what a Target app drew into regions: the app's
//! interface, Agent output paragraphs, code, lists, tables and the Input
//! box.
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

    /// True when one of `rows` begins with the mark this app starts a reply
    /// with. No other program draws that mark there, so a screen that shows
    /// it is this app's.
    pub fn shows_a_reply_mark(&self, rows: &[Row]) -> bool {
        let starts_a_reply = match self {
            Finder::ClaudeCode(_) => claude_code::starts_a_reply,
            Finder::Codex(_) => codex::starts_a_reply,
        };
        rows.iter().any(|row| starts_a_reply(row))
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// One paragraph of Agent output, row by row as the app wrapped it. A
    /// line that the writer ended with a line break is a paragraph of its
    /// own.
    Paragraph(ParagraphStart),
    /// A code block. Code is always left to right and stays as drawn.
    Code,
    /// List items, with the rows they wrap onto.
    List,
    /// A table with its borders.
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

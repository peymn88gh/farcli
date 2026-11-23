//! Region finder: splits what a Target app drew into regions: the app's
//! interface, Agent output paragraphs, code, lists, tables and the Input
//! box.
//!
//! How a screen is read depends on the Target app, so each app has its own
//! finder. They all answer in the terms of this file. Rows that are in no
//! region are the app's interface, which is never touched, and so is a
//! region the finder could not name with confidence.

pub(crate) mod claude_code;

use std::ops::Range;

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
    /// One paragraph of Agent output, row by row as the app wrapped it.
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
    /// It began above the first row given: its first rows have scrolled
    /// out of sight.
    Above,
}

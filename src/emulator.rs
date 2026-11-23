//! Plumbing: the library that turns drawing codes into a grid of cells.
//! Nothing outside this file names the library's types.

use std::sync::Arc;

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell as GridCell, Flags};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{self, NamedColor, Processor};

use crate::screen::{Cell, CellWidth, Color, Link, Position, Row, Style, Underline};
use crate::Size;

/// A grid of cells that obeys drawing codes the way a Terminal does.
pub(crate) struct Emulator {
    term: Term<VoidListener>,
    processor: Processor,
}

struct GridSize(Size);

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        usize::from(self.0.rows)
    }

    fn columns(&self) -> usize {
        usize::from(self.0.cols)
    }
}

impl Emulator {
    /// `scrollback_rows` is how many rows that scrolled off the top are kept.
    pub fn new(size: Size, scrollback_rows: usize) -> Emulator {
        let config = Config {
            scrolling_history: scrollback_rows,
            ..Config::default()
        };
        Emulator {
            term: Term::new(config, &GridSize(size), VoidListener),
            processor: Processor::new(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }

    pub fn size(&self) -> Size {
        Size {
            cols: self.term.columns() as u16,
            rows: self.term.screen_lines() as u16,
        }
    }

    pub fn resize(&mut self, size: Size) {
        self.term.resize(GridSize(size));
    }

    pub fn cursor(&self) -> Position {
        let point = self.term.grid().cursor.point;
        Position {
            row: point.line.0.max(0) as u16,
            col: point.column.0 as u16,
        }
    }

    pub fn cursor_visible(&self) -> bool {
        self.term.mode().contains(TermMode::SHOW_CURSOR)
    }

    pub fn on_alternate_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    pub fn rows(&self) -> Vec<Row> {
        (0..self.term.screen_lines() as i32)
            .map(|line| self.line(Line(line)))
            .collect()
    }

    pub fn scrollback_len(&self) -> usize {
        self.term.grid().history_size()
    }

    /// The rows that scrolled off the top, oldest first.
    pub fn scrollback(&self) -> Vec<Row> {
        let len = self.scrollback_len() as i32;
        (1..=len)
            .rev()
            .map(|above| self.line(Line(-above)))
            .collect()
    }

    pub fn forget_scrollback(&mut self) {
        self.term.grid_mut().clear_history();
    }

    fn line(&self, line: Line) -> Row {
        let grid = self.term.grid();
        (0..self.term.columns())
            .map(|column| cell(&grid[line][Column(column)]))
            .collect()
    }
}

fn cell(from: &GridCell) -> Cell {
    let width = if from.flags.contains(Flags::WIDE_CHAR) {
        CellWidth::Two
    } else if from.flags.contains(Flags::WIDE_CHAR_SPACER) {
        CellWidth::SecondHalf
    } else {
        CellWidth::One
    };
    let underline = if from.flags.contains(Flags::UNDERLINE) {
        Underline::Single
    } else if from.flags.contains(Flags::DOUBLE_UNDERLINE) {
        Underline::Double
    } else if from.flags.contains(Flags::UNDERCURL) {
        Underline::Curly
    } else if from.flags.contains(Flags::DOTTED_UNDERLINE) {
        Underline::Dotted
    } else if from.flags.contains(Flags::DASHED_UNDERLINE) {
        Underline::Dashed
    } else {
        Underline::None
    };
    Cell {
        character: from.c,
        marks: from.zerowidth().map(<[char]>::to_vec).unwrap_or_default(),
        width,
        style: Style {
            foreground: color(from.fg),
            background: color(from.bg),
            bold: from.flags.contains(Flags::BOLD),
            dim: from.flags.contains(Flags::DIM),
            italic: from.flags.contains(Flags::ITALIC),
            underline,
            underline_color: from.underline_color().map(color),
            inverse: from.flags.contains(Flags::INVERSE),
            hidden: from.flags.contains(Flags::HIDDEN),
            strikeout: from.flags.contains(Flags::STRIKEOUT),
            link: from.hyperlink().map(|link| {
                Arc::new(Link {
                    id: link.id().to_owned(),
                    uri: link.uri().to_owned(),
                })
            }),
        },
    }
}

fn color(from: ansi::Color) -> Color {
    match from {
        ansi::Color::Named(named) => match named {
            NamedColor::Black => Color::Palette(0),
            NamedColor::Red => Color::Palette(1),
            NamedColor::Green => Color::Palette(2),
            NamedColor::Yellow => Color::Palette(3),
            NamedColor::Blue => Color::Palette(4),
            NamedColor::Magenta => Color::Palette(5),
            NamedColor::Cyan => Color::Palette(6),
            NamedColor::White => Color::Palette(7),
            NamedColor::BrightBlack => Color::Palette(8),
            NamedColor::BrightRed => Color::Palette(9),
            NamedColor::BrightGreen => Color::Palette(10),
            NamedColor::BrightYellow => Color::Palette(11),
            NamedColor::BrightBlue => Color::Palette(12),
            NamedColor::BrightMagenta => Color::Palette(13),
            NamedColor::BrightCyan => Color::Palette(14),
            NamedColor::BrightWhite => Color::Palette(15),
            _ => Color::Default,
        },
        ansi::Color::Indexed(index) => Color::Palette(index),
        ansi::Color::Spec(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

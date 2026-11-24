//! What a screen is made of: rows of cells, each with a character and a
//! style. The Hidden screen, the Painter and the fake Terminal all speak in
//! these terms.

use std::sync::Arc;

/// A place on a screen. Row 0 is the top row and column 0 the left column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub row: u16,
    pub col: u16,
}

/// One row of a screen, left to right in Screen order.
pub type Row = Vec<Cell>;

/// One character cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub character: char,
    /// Characters that take no cell of their own and are drawn with
    /// `character`, such as a Half-space or an accent.
    pub marks: Vec<char>,
    pub width: CellWidth,
    pub style: Style,
}

impl Cell {
    /// An empty cell in the Terminal's own colours.
    pub fn blank() -> Cell {
        Cell {
            character: ' ',
            marks: Vec::new(),
            width: CellWidth::One,
            style: Style::default(),
        }
    }

    pub fn is_blank(&self) -> bool {
        *self == Cell::blank()
    }

    /// A cell the Target app has not drawn since farcli started. The
    /// Terminal still shows there whatever it showed before, and farcli
    /// leaves it alone.
    pub fn left_alone() -> Cell {
        Cell {
            character: '\0',
            ..Cell::blank()
        }
    }

    pub fn is_left_alone(&self) -> bool {
        self.character == '\0'
    }
}

/// How a cell takes part in the width of its character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellWidth {
    /// The character fills this cell only.
    One,
    /// The character fills this cell and the next one.
    Two,
    /// The second cell of the character in the cell before.
    SecondHalf,
}

/// Everything about how a cell is drawn apart from its character.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Style {
    pub foreground: Color,
    pub background: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: Underline,
    /// `None` draws the underline in the foreground colour.
    pub underline_color: Option<Color>,
    pub inverse: bool,
    pub hidden: bool,
    pub strikeout: bool,
    pub link: Option<Arc<Link>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    /// The Terminal's own foreground or background colour.
    #[default]
    Default,
    /// One of the Terminal's 256 numbered colours.
    Palette(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Underline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

/// A clickable link carried by a cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Cells with the same id belong to the same link.
    pub id: String,
    pub uri: String,
}

/// Something that happened to a whole screen, beyond cells changing in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenChange {
    /// Every row moved up by one and this row left the screen at the top.
    ScrolledOff(Row),
    /// The Target app cleared part of the screen while its cursor was at
    /// `cursor`.
    Erased { part: EraseScreen, cursor: Position },
    /// The Target app moved to the alternate screen that full-screen
    /// programs draw on (`true`), or back to the main screen (`false`).
    AlternateScreen(bool),
}

/// The part of a screen a Target app asked to clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraseScreen {
    /// From the cursor to the end of the screen.
    Below,
    /// From the start of the screen to the cursor.
    Above,
    /// The whole screen.
    All,
    /// The rows that scrolled off the top.
    Scrollback,
}

/// The text of a row with the empty cells at its right end left off.
pub fn row_text(row: &[Cell]) -> String {
    let mut text = String::new();
    for cell in row {
        if cell.width == CellWidth::SecondHalf {
            continue;
        }
        text.push(if cell.is_left_alone() {
            ' '
        } else {
            cell.character
        });
        text.extend(&cell.marks);
    }
    text.truncate(text.trim_end_matches(' ').len());
    text
}
